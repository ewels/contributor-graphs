use crate::model::{Commit, CommitFilter, Release};
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
    /// True for a cloned remote (has an `origin` to fetch from); false for a
    /// local checkout. Drives whether freshness is checked via `git ls-remote`.
    pub is_remote: bool,
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
        is_remote: false,
    })
}

/// Where cached clones live: under the tool's XDG cache dir, falling back to a
/// temp directory if no home/XDG dir is available.
fn clones_dir() -> PathBuf {
    crate::cache::root()
        .unwrap_or_else(std::env::temp_dir)
        .join("clones")
}

fn prepare_remote(
    clone_url: &str,
    slug: Option<String>,
    branch: Option<&str>,
) -> Result<PreparedRepo> {
    let cache_key = sanitize(slug.as_deref().unwrap_or(clone_url));
    let cache_dir = clones_dir().join(cache_key);

    // Clone on first sight only. Updating an existing clone is deferred to
    // fetch(), which the caller calls just for repos whose history changed.
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
        is_remote: true,
    })
}

/// The current tip SHA of `branch` on the remote, read with `git ls-remote`
/// (refs only, no object transfer). Used as a cheap freshness token without
/// fetching. `None` if the repo is local or the remote can't be reached.
pub fn remote_tip(repo: &PreparedRepo) -> Option<String> {
    if !repo.is_remote {
        return None;
    }
    let out = git(&repo.git_dir, &["ls-remote", "origin", &repo.branch]).ok()?;
    out.split_whitespace().next().map(str::to_string)
}

/// The tip SHA of `branch` in the local clone/checkout.
pub fn local_tip(repo: &PreparedRepo) -> Option<String> {
    git(&repo.git_dir, &["rev-parse", &repo.branch]).ok()
}

/// Update a cached clone from its origin. Called only when the history is known
/// to have changed; returns whether the fetch succeeded.
pub fn fetch(repo: &PreparedRepo) -> bool {
    eprintln!("  updating cached clone {}", repo.git_dir.display());
    git(
        &repo.git_dir,
        &[
            "fetch",
            "--quiet",
            "--prune",
            "origin",
            "+refs/heads/*:refs/heads/*",
            // Keep tags current too, so release markers reflect new releases.
            "+refs/tags/*:refs/tags/*",
        ],
    )
    .is_ok()
}

/// Read every git tag with its date (tag date for annotated tags, commit date
/// for lightweight ones), sorted oldest-first. Used to mark releases on the
/// timeline. A local-only operation — no network — so it's cheap to call.
pub fn read_tags(repo: &PreparedRepo) -> Vec<Release> {
    let out = match git(
        &repo.git_dir,
        &[
            "for-each-ref",
            "--sort=creatordate",
            "--format=%(refname:short)%09%(creatordate:unix)",
            "refs/tags",
        ],
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    out.lines()
        .filter_map(|line| {
            let (name, ts) = line.split_once('\t')?;
            let ts: i64 = ts.trim().parse().ok()?;
            let name = name.trim();
            (!name.is_empty() && ts > 0).then(|| Release {
                name: name.to_string(),
                ts,
            })
        })
        .collect()
}

fn resolve_branch(dir: &Path, requested: Option<&str>) -> String {
    if let Some(b) = requested {
        return b.to_string();
    }
    git(dir, &["symbolic-ref", "--short", "HEAD"]).unwrap_or_else(|_| "HEAD".into())
}

/// Run `git log` and parse one commit per record. Records are separated by a
/// record-separator byte (\x1e) so a commit's multi-value `Co-authored-by`
/// trailers (joined with \x1f) can't be confused with the next commit.
pub fn read_commits(
    repo: &PreparedRepo,
    branch: Option<&str>,
    filter: &CommitFilter,
) -> Result<Vec<Commit>> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(&repo.git_dir).args([
        "log",
        "--use-mailmap",
        "--pretty=format:%x1e%H%x09%at%x09%aN%x09%aE%x09\
         %(trailers:key=Co-authored-by,valueonly,separator=%x1f)",
    ]);
    if filter.no_merges {
        cmd.arg("--no-merges");
    }
    if let Some(s) = &filter.since {
        cmd.arg(format!("--since={s}"));
    }
    if let Some(u) = &filter.until {
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
    for rec in text.split('\u{1e}') {
        let mut parts = rec.splitn(5, '\t');
        let (Some(sha), Some(ts), Some(name), Some(email)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Ok(ts) = ts.parse::<i64>() else { continue };
        let email = email.trim().to_lowercase();
        let coauthors = parts
            .next()
            .into_iter()
            .flat_map(|block| block.split('\u{1f}'))
            .filter_map(parse_coauthor)
            .filter(|(_, e)| e != &email)
            .collect();
        commits.push(Commit {
            sha: sha.to_string(),
            ts,
            name: name.trim().to_string(),
            email,
            coauthors,
            src: 0,
        });
    }
    Ok(commits)
}

/// Parse a `Co-authored-by` value (`Name <email>`) into `(name, email)`.
fn parse_coauthor(raw: &str) -> Option<(String, String)> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    match (s.find('<'), s.rfind('>')) {
        (Some(lt), Some(gt)) if gt > lt => {
            let name = s[..lt].trim().to_string();
            let email = s[lt + 1..gt].trim().to_lowercase();
            (!email.is_empty() || !name.is_empty()).then_some((name, email))
        }
        _ => Some((s.to_string(), String::new())),
    }
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

/// A bare GitHub owner name (org or user): a single token, not a local path,
/// not a slug, not a URL. Used to decide whether to expand the input into all
/// of that owner's repositories.
pub fn looks_like_owner(input: &str) -> bool {
    let s = input.trim();
    if s.is_empty() || s.contains('/') || s.contains(':') || s.contains('.') {
        return false;
    }
    if Path::new(s).exists() {
        return false;
    }
    !s.starts_with('-') && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
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
