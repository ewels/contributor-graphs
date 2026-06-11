use crate::model::Commit;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct PreparedRepo {
    pub git_dir: PathBuf,
    /// `owner/repo` when the repository lives on GitHub.
    pub slug: Option<String>,
    /// Web URL for the repository, when known.
    pub url: Option<String>,
    pub display_name: String,
    pub branch: String,
}

/// Resolve user input (local path, `owner/repo` slug, or git URL) into a local
/// git directory we can run `git log` against. Remote repos are cloned as
/// bare, treeless partial clones into a cache directory.
pub fn prepare(input: &str, branch: Option<&str>) -> Result<PreparedRepo> {
    let path = Path::new(input);
    if path.exists() {
        return prepare_local(path, branch);
    }

    if let Some(slug) = parse_github_url(input) {
        let url = format!("https://github.com/{slug}");
        return prepare_remote(&format!("{url}.git"), Some(slug), branch);
    }
    if looks_like_slug(input) {
        let url = format!("https://github.com/{input}.git");
        return prepare_remote(&url, Some(input.to_string()), branch);
    }
    if input.contains("://") || input.starts_with("git@") {
        return prepare_remote(input, None, branch);
    }

    bail!(
        "'{input}' is not a local path, an owner/repo GitHub slug, or a git URL.\n\
         Examples: contributor-graphs .  |  contributor-graphs nf-core/rnaseq  |  \
         contributor-graphs https://github.com/MultiQC/MultiQC"
    )
}

fn prepare_local(path: &Path, branch: Option<&str>) -> Result<PreparedRepo> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("cannot resolve path {}", path.display()))?;
    let ok = git(&canonical, &["rev-parse", "--git-dir"]).is_ok();
    if !ok {
        bail!("{} is not a git repository", canonical.display());
    }
    let remote = git(&canonical, &["remote", "get-url", "origin"]).ok();
    let slug = remote.as_deref().and_then(parse_github_url);
    let display_name = slug.clone().unwrap_or_else(|| {
        canonical
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "repository".into())
    });
    let url = slug.as_ref().map(|s| format!("https://github.com/{s}"));
    let branch = resolve_branch(&canonical, branch);
    Ok(PreparedRepo {
        git_dir: canonical,
        slug,
        url,
        display_name,
        branch,
    })
}

fn prepare_remote(
    clone_url: &str,
    slug: Option<String>,
    branch: Option<&str>,
) -> Result<PreparedRepo> {
    let cache_key = sanitize(slug.as_deref().unwrap_or(clone_url));
    let cache_dir = std::env::temp_dir()
        .join("contributor-graphs")
        .join(cache_key);

    if cache_dir.join("HEAD").exists() {
        eprintln!("  updating cached clone {}", cache_dir.display());
        let fetched = git(
            &cache_dir,
            &[
                "fetch",
                "--quiet",
                "--prune",
                "origin",
                "+refs/heads/*:refs/heads/*",
            ],
        )
        .is_ok();
        if !fetched {
            eprintln!("  fetch failed, re-cloning");
            let _ = std::fs::remove_dir_all(&cache_dir);
        }
    }
    if !cache_dir.join("HEAD").exists() {
        std::fs::create_dir_all(cache_dir.parent().unwrap()).ok();
        eprintln!("  cloning {clone_url} (commit history only)");
        let status = Command::new("git")
            .args(["clone", "--bare", "--filter=tree:0", "--quiet", clone_url])
            .arg(&cache_dir)
            .status()
            .context("failed to run git clone")?;
        if !status.success() {
            bail!("git clone of {clone_url} failed");
        }
        // Keep the remote HEAD up to date on later fetches.
        let _ = git(&cache_dir, &["remote", "set-head", "origin", "--auto"]);
    }

    let display_name = slug.clone().unwrap_or_else(|| {
        clone_url
            .trim_end_matches(".git")
            .rsplit('/')
            .next()
            .unwrap_or("repository")
            .to_string()
    });
    let url = slug.as_ref().map(|s| format!("https://github.com/{s}"));
    let branch = resolve_branch(&cache_dir, branch);
    Ok(PreparedRepo {
        git_dir: cache_dir,
        slug,
        url,
        display_name,
        branch,
    })
}

fn resolve_branch(dir: &Path, requested: Option<&str>) -> String {
    if let Some(b) = requested {
        return b.to_string();
    }
    git(dir, &["symbolic-ref", "--short", "HEAD"]).unwrap_or_else(|_| "HEAD".into())
}

/// Run `git log` and parse one commit per line.
pub fn read_commits(
    repo: &PreparedRepo,
    branch: Option<&str>,
    since: Option<&str>,
    until: Option<&str>,
    no_merges: bool,
) -> Result<Vec<Commit>> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(&repo.git_dir).args([
        "log",
        "--use-mailmap",
        "--pretty=format:%H%x09%at%x09%aN%x09%aE",
    ]);
    if no_merges {
        cmd.arg("--no-merges");
    }
    if let Some(s) = since {
        cmd.arg(format!("--since={s}"));
    }
    if let Some(u) = until {
        cmd.arg(format!("--until={u}"));
    }
    cmd.arg(branch.unwrap_or("HEAD")).arg("--");

    let out = cmd.output().context("failed to run git log")?;
    if !out.status.success() {
        bail!(
            "git log failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut commits = Vec::new();
    for line in text.lines() {
        let mut parts = line.splitn(4, '\t');
        let (Some(sha), Some(ts), Some(name), Some(email)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Ok(ts) = ts.parse::<i64>() else { continue };
        commits.push(Commit {
            sha: sha.to_string(),
            ts,
            name: name.trim().to_string(),
            email: email.trim().to_lowercase(),
        });
    }
    Ok(commits)
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    if !out.status.success() {
        bail!("git {:?} failed", args);
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Extract `owner/repo` from common GitHub URL shapes.
pub fn parse_github_url(url: &str) -> Option<String> {
    let u = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let rest = u
        .strip_prefix("git@github.com:")
        .or_else(|| u.strip_prefix("https://github.com/"))
        .or_else(|| u.strip_prefix("http://github.com/"))
        .or_else(|| u.strip_prefix("ssh://git@github.com/"))
        .or_else(|| u.strip_prefix("github.com/"))?;
    let mut parts = rest.splitn(3, '/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

fn looks_like_slug(s: &str) -> bool {
    let parts: Vec<&str> = s.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

pub fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}
