//! Build contributor timelines from a git or GitHub repository.
//!
//! `contributor-graphs` is primarily a command-line tool, but the same engine
//! is available as a library. The usual flow is [`analyze`] to turn a
//! repository into [`Contributor`] rows plus [`RepoMeta`], then one of the
//! renderers in [`svg`] or [`html`].
//!
//! ```no_run
//! use contributor_graphs::{analyze, svg, Config};
//!
//! let analysis = analyze("nf-core/rnaseq", &Config::default())?;
//! let rows: Vec<_> = analysis.contributors.iter().filter(|c| !c.bot).cloned().collect();
//! let opts = svg::SvgOptions {
//!     title: analysis.meta.name.clone(),
//!     ..Default::default()
//! };
//! std::fs::write("rnaseq.svg", svg::render_svg(&rows, &opts))?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! The lower-level modules ([`repo`], [`identity`], [`github`]) are public too,
//! for callers who want to assemble a custom pipeline.

pub mod cache;
pub mod github;
pub mod html;
pub mod identity;
pub mod model;
pub mod repo;
pub mod svg;
pub mod theme;

use anyhow::{bail, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub use model::{Contributor, RepoMeta};

/// Worker threads for reading per-source history in parallel.
const READ_THREADS: usize = 8;

/// How to read history and resolve identities. Construct with `Config::default()`
/// and override fields as needed.
#[derive(Clone)]
pub struct Config {
    /// Branch or ref to read (default: `HEAD`).
    pub branch: Option<String>,
    /// Only include commits after this date (passed to `git log --since`).
    pub since: Option<String>,
    /// Only include commits before this date (`git log --until`).
    pub until: Option<String>,
    /// Skip merge commits.
    pub no_merges: bool,
    /// Override the chart/repository title.
    pub title: Option<String>,
    /// Exclude contributors whose name or login contains any of these strings.
    pub exclude: Vec<String>,
    /// Manual `matcher → group` mappings (matcher = name, email, or login).
    pub groups: Vec<(String, String)>,
    /// Manual identity merges: each row is `[canonical, alias, …]`.
    pub identities: Vec<Vec<String>>,
    /// Query the GitHub API for logins, avatars, and profiles.
    pub use_github: bool,
    /// Auto-detect affiliations from GitHub profile companies.
    pub detect_affiliation: bool,
    /// Merge identities that share a normalised author name.
    pub merge_names: bool,
    /// Download avatars and embed them as data URIs.
    pub embed_avatars: bool,
    /// Avatar pixel size to request when embedding.
    pub avatar_size: u32,
    /// Ignore cached git history and GitHub lookups, forcing a fresh pull
    /// (the caches are still refreshed with the new results).
    pub refresh: bool,
    /// Print progress to stderr.
    pub verbose: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            branch: None,
            since: None,
            until: None,
            no_merges: false,
            title: None,
            exclude: Vec::new(),
            groups: Vec::new(),
            identities: Vec::new(),
            use_github: true,
            detect_affiliation: true,
            merge_names: true,
            embed_avatars: true,
            avatar_size: 64,
            refresh: false,
            verbose: false,
        }
    }
}

/// The result of [`analyze`]: every contributor (bots included — filter on
/// [`Contributor::bot`] if you don't want them) and repository metadata.
pub struct Analysis {
    pub contributors: Vec<Contributor>,
    pub meta: RepoMeta,
}

/// Row ordering for [`sort`].
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Sort {
    /// Oldest first commit at the top.
    First,
    /// Most recent commit first.
    Last,
    /// Most commits first.
    Commits,
    /// Longest active period first.
    Duration,
    /// Alphabetical by name.
    Name,
}

/// Sort contributor rows in place.
pub fn sort(rows: &mut [Contributor], key: Sort) {
    match key {
        Sort::First => rows.sort_by(|a, b| a.first.cmp(&b.first).then(b.commits.cmp(&a.commits))),
        Sort::Last => rows.sort_by(|a, b| b.last.cmp(&a.last).then(b.commits.cmp(&a.commits))),
        Sort::Commits => rows.sort_by_key(|c| std::cmp::Reverse(c.commits)),
        Sort::Duration => rows.sort_by_key(|c| std::cmp::Reverse(c.last - c.first)),
        Sort::Name => rows.sort_by_key(|a| a.name.to_lowercase()),
    }
}

/// Resolve a single repository (local path, `owner/repo` slug, or git URL)
/// into contributor data and metadata. Shorthand for [`analyze_many`] with one
/// source.
pub fn analyze(input: &str, cfg: &Config) -> Result<Analysis> {
    analyze_many(std::slice::from_ref(&input), cfg)
}

/// Resolve one or more repositories into a single combined timeline. Commits
/// from every source are pooled, author identities are clustered across the
/// whole pool, and commits that appear in more than one source (overlapping
/// histories) are de-duplicated by commit SHA. Disjoint sources contribute
/// distinct SHAs and so are simply concatenated.
pub fn analyze_many(inputs: &[&str], cfg: &Config) -> Result<Analysis> {
    macro_rules! log {
        ($($arg:tt)*) => { if cfg.verbose { eprintln!($($arg)*); } };
    }
    if inputs.is_empty() {
        bail!("no repository sources given");
    }

    let client = github::GhClient::new(if cfg.use_github {
        github::find_token()
    } else {
        None
    });
    let now = chrono::Utc::now().timestamp();
    let mut caches = cache::Caches::load(cfg.refresh, now);

    // Expand any bare owner names (a single token that is not a local path,
    // slug, or URL) into every repository under that GitHub org or user.
    // Everything else passes through unchanged. Explicit repos and whole orgs
    // can be mixed freely; overlaps are dropped later by commit SHA.
    let mut sources: Vec<String> = Vec::new();
    for input in inputs {
        if repo::looks_like_owner(input) {
            if !cfg.use_github {
                bail!("'{input}' looks like an org/user, but listing its repositories needs GitHub access (remove --no-github, or pass owner/repo slugs)");
            }
            let (slugs, cached) = match caches.org_repos(input) {
                Some(repos) => (repos, true),
                None => {
                    log!("→ listing repositories for '{input}'");
                    let fetched = client.list_owner_repos(input);
                    if !fetched.is_empty() {
                        caches.put_org_repos((*input).to_string(), fetched.clone());
                    }
                    (fetched, false)
                }
            };
            if slugs.is_empty() {
                if inputs.len() == 1 {
                    bail!("no repositories found for org/user '{input}' (it may not exist or has no non-fork repos)");
                }
                log!("  warning: no repositories found for '{input}'");
            } else {
                log!(
                    "  {} repositories{}",
                    slugs.len(),
                    if cached { " (cached)" } else { "" }
                );
                sources.extend(slugs);
            }
        } else {
            sources.push((*input).to_string());
        }
    }
    if sources.is_empty() {
        bail!("no usable repository sources");
    }

    // Prepare each source. With several sources, a single repo that fails to
    // clone or has no commits shouldn't abort the whole pool: skip it with a
    // warning. A single-source run still surfaces the error.
    let mut prepared: Vec<repo::PreparedRepo> = Vec::new();
    for input in &sources {
        match repo::prepare(input, cfg.branch.as_deref()) {
            Ok(p) => prepared.push(p),
            Err(e) if sources.len() > 1 => log!("  warning: skipping source '{input}' ({e})"),
            Err(e) => return Err(e),
        }
    }
    if prepared.is_empty() {
        bail!("no usable repository sources");
    }
    let source_slugs: Vec<Option<String>> = prepared.iter().map(|p| p.slug.clone()).collect();
    for p in &prepared {
        log!("→ source: {} (branch {})", p.display_name, p.branch);
    }

    let filter = model::CommitFilter {
        since: cfg.since.clone(),
        until: cfg.until.clone(),
        no_merges: cfg.no_merges,
    };
    let branch = cfg.branch.as_deref();

    // Read each source's history, reusing the cached `git log` when the branch
    // tip is unchanged. Sources are independent, so read them in parallel and
    // merge in source order afterwards, which keeps de-duplication deterministic
    // (an earlier source keeps a SHA it shares with a later one).
    let outcomes: Vec<Mutex<Option<Result<SourceRead>>>> =
        (0..prepared.len()).map(|_| Mutex::new(None)).collect();
    let cursor = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..READ_THREADS.min(prepared.len()) {
            s.spawn(|| loop {
                let i = cursor.fetch_add(1, Ordering::Relaxed);
                let Some(p) = prepared.get(i) else { break };
                let r = read_source(p, &caches, &filter, branch);
                *outcomes[i].lock().unwrap() = Some(r);
            });
        }
    });

    let mut commits: Vec<model::Commit> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut duplicates = 0u64;
    let mut cached_sources = 0usize;
    for (i, (p, slot)) in prepared.iter().zip(outcomes).enumerate() {
        let read = match slot.into_inner().unwrap() {
            Some(Ok(r)) => r,
            Some(Err(e)) if prepared.len() > 1 => {
                log!("  warning: skipping {} ({e})", p.display_name);
                continue;
            }
            Some(Err(e)) => return Err(e),
            None => continue,
        };
        if read.from_cache {
            cached_sources += 1;
        }
        for mut c in read.commits {
            if !seen.insert(c.sha.clone()) {
                duplicates += 1;
                continue;
            }
            c.src = i as u32;
            commits.push(c);
        }
    }
    if commits.is_empty() {
        bail!("no commits found");
    }
    if cached_sources > 0 {
        log!(
            "→ reused cached history for {cached_sources}/{} sources",
            prepared.len()
        );
    }
    if prepared.len() > 1 {
        log!(
            "→ {} commits from {} sources ({} duplicate commits dropped), {} distinct author emails",
            model::thousands(commits.len() as u64),
            prepared.len(),
            model::thousands(duplicates),
            distinct_emails(&commits)
        );
    } else {
        log!(
            "→ {} commits from {} distinct author emails",
            model::thousands(commits.len() as u64),
            distinct_emails(&commits)
        );
    }

    let mut clusters = identity::cluster_commits(&commits, cfg.merge_names);

    let any_slug = source_slugs.iter().any(|s| s.is_some());
    if cfg.use_github {
        if any_slug {
            log!("→ enriching from GitHub");
            github::enrich_clusters(
                &mut clusters,
                &commits,
                &source_slugs,
                &client,
                &mut caches,
                cfg.verbose,
            );
            clusters = identity::merge_by_login(clusters);
            github::fetch_profiles(&mut clusters, &client, &mut caches, cfg.verbose);
            if !cfg.detect_affiliation {
                for cl in clusters.iter_mut() {
                    cl.affiliation = None;
                }
            }
        } else {
            log!("→ no GitHub sources, skipping enrichment");
        }
    }

    if !cfg.identities.is_empty() {
        clusters = identity::apply_identity_file(clusters, &cfg.identities);
        log!("→ applied {} identity overrides", cfg.identities.len());
    }

    let mut contributors = identity::build_contributors(&clusters, &commits, &cfg.groups);

    let n_groups = canonicalize_groups(&mut contributors);
    if n_groups > 0 {
        log!("→ {n_groups} distinct affiliations/groups");
    }

    if !cfg.exclude.is_empty() {
        contributors.retain(|c| {
            !cfg.exclude.iter().any(|pat| {
                let p = pat.to_lowercase();
                c.name.to_lowercase().contains(&p)
                    || c.login
                        .as_deref()
                        .is_some_and(|l| l.to_lowercase().contains(&p))
            })
        });
    }

    log!(
        "→ merged to {} contributors ({} bots)",
        contributors.len(),
        contributors.iter().filter(|c| c.bot).count()
    );

    if cfg.embed_avatars && cfg.use_github {
        github::embed_avatars(
            &mut contributors,
            &client,
            &mut caches,
            cfg.avatar_size,
            cfg.verbose,
        );
    }

    // A single source keeps its slug/url/branch; multiple sources collapse to a
    // combined label with no single canonical URL.
    let single = if prepared.len() == 1 {
        Some(&prepared[0])
    } else {
        None
    };

    // The GitHub owner shared by every source, if any (a single repo or a
    // same-owner pool such as a whole-org timeline).
    let owner = common_owner(&prepared);

    // Owner/org avatar for the interactive page header. Shown for a single
    // GitHub source and for a same-owner pool, but not for a mix of owners.
    let owner_avatar = if cfg.use_github && cfg.embed_avatars {
        owner
            .as_deref()
            .and_then(|owner| github::fetch_avatar(&client, &mut caches, owner, 48))
    } else {
        None
    };

    // Repository description (single GitHub source).
    let description = if cfg.use_github {
        single
            .and_then(|p| p.slug.as_deref())
            .and_then(|slug| github::fetch_repo_description(&client, slug))
    } else {
        None
    };

    // A single repo keeps its name; a same-owner pool is labelled by the owner
    // (so a whole org reads as "nf-core"); a mixed pool joins the repo names.
    let default_name = match (single, &owner) {
        (Some(p), _) => p.display_name.clone(),
        (None, Some(owner)) => owner.clone(),
        (None, None) => combined_name(&prepared),
    };
    let branch = match single {
        Some(p) => p.branch.clone(),
        None => "combined".to_string(),
    };

    let first = contributors.iter().map(|c| c.first).min().unwrap_or(0);
    let last = contributors.iter().map(|c| c.last).max().unwrap_or(0);
    let meta = RepoMeta {
        name: cfg.title.clone().unwrap_or(default_name),
        url: single.and_then(|p| p.url.clone()),
        slug: single.and_then(|p| p.slug.clone()),
        branch,
        first,
        last,
        total_commits: commits.len() as u64,
        total_contributors: contributors.iter().filter(|c| !c.bot).count(),
        generated: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        owner_avatar,
        description,
    };

    caches.save();
    Ok(Analysis { contributors, meta })
}

struct SourceRead {
    commits: Vec<model::Commit>,
    from_cache: bool,
}

/// Read one source's history, reusing the cached `git log` when the branch tip
/// is unchanged. On a miss, the clone is brought up to date, parsed, and the
/// result is written back to the cache against the new tip.
fn read_source(
    p: &repo::PreparedRepo,
    caches: &cache::Caches,
    filter: &model::CommitFilter,
    branch: Option<&str>,
) -> Result<SourceRead> {
    let key = source_cache_key(p);
    // Freshness token: the remote tip via `ls-remote` (no object transfer),
    // falling back to the local tip when offline or for a local checkout.
    let remote = repo::remote_tip(p);
    let tip = remote.clone().or_else(|| repo::local_tip(p));

    if let Some(tip) = &tip {
        if let Some(cached) = caches.commits(&key, tip, filter) {
            let commits = cached
                .into_iter()
                .map(|c| model::Commit {
                    sha: c.sha,
                    ts: c.ts,
                    name: c.name,
                    email: c.email,
                    src: 0,
                })
                .collect();
            return Ok(SourceRead {
                commits,
                from_cache: true,
            });
        }
    }

    // Cache miss. Update the clone only when it is genuinely behind the remote;
    // a just-cloned repo or an offline run reads what is already on disk.
    let local = repo::local_tip(p);
    if p.is_remote && remote.is_some() && remote != local {
        repo::fetch(p);
    }
    let commits = repo::read_commits(p, branch, filter)?;
    // After a fetch the tip moved, so re-read it; otherwise the pre-fetch local
    // tip still stands.
    if let Some(tip) = repo::local_tip(p) {
        let cached = commits
            .iter()
            .map(|c| cache::CachedCommit {
                sha: c.sha.clone(),
                ts: c.ts,
                name: c.name.clone(),
                email: c.email.clone(),
            })
            .collect();
        caches.put_commits(&key, &tip, filter, cached);
    }
    Ok(SourceRead {
        commits,
        from_cache: false,
    })
}

/// Stable per-repo cache key: the slug (or display name) plus the branch.
fn source_cache_key(p: &repo::PreparedRepo) -> String {
    let base = p.slug.as_deref().unwrap_or(&p.display_name);
    repo::sanitize(&format!("{base}__{}", p.branch))
}

/// Build a chart title for a multi-source run: join up to three source names
/// with " + ", and summarise the rest as "+N more".
fn combined_name(prepared: &[repo::PreparedRepo]) -> String {
    let names: Vec<&str> = prepared.iter().map(|p| p.display_name.as_str()).collect();
    match names.len() {
        0 => "repositories".to_string(),
        1..=3 => names.join(" + "),
        n => format!("{} + {} more", names[..2].join(" + "), n - 2),
    }
}

/// The GitHub owner shared by every source, if all sources are GitHub slugs
/// under one owner. Returns `None` if any source has no slug or the owners
/// differ, so a mixed-owner run falls back to the generic header icon.
fn common_owner(prepared: &[repo::PreparedRepo]) -> Option<String> {
    let mut owner: Option<String> = None;
    for p in prepared {
        let o = p.slug.as_deref()?.split('/').next()?.to_string();
        match &owner {
            Some(prev) if *prev != o => return None,
            _ => owner = Some(o),
        }
    }
    owner
}

fn distinct_emails(commits: &[model::Commit]) -> usize {
    let mut e: Vec<&str> = commits.iter().map(|c| c.email.as_str()).collect();
    e.sort_unstable();
    e.dedup();
    e.len()
}

/// Merge group-name variants that refer to the same organisation:
/// case/punctuation differences ("Seqera Labs" vs "seqeralabs"), a leading
/// "The", and prefix forms ("Seqera" vs "Seqera Labs"). Returns the final
/// group count.
fn canonicalize_groups(contributors: &mut [Contributor]) -> usize {
    use std::collections::HashMap;
    let alnum_key = |g: &str| -> String {
        let lower = g.to_lowercase();
        let trimmed = lower.strip_prefix("the ").unwrap_or(&lower);
        trimmed.chars().filter(|c| c.is_alphanumeric()).collect()
    };

    let mut variants: HashMap<String, usize> = HashMap::new();
    for c in contributors.iter() {
        if let Some(g) = &c.group {
            *variants.entry(g.clone()).or_default() += 1;
        }
    }

    let mut keys: Vec<String> = variants.keys().map(|g| alnum_key(g)).collect();
    keys.sort();
    keys.dedup();
    let resolve = |key: &str| -> String {
        keys.iter()
            .filter(|k| k.len() >= 6 && key.starts_with(*k))
            .min_by_key(|k| k.len())
            .map(|k| k.to_string())
            .unwrap_or_else(|| key.to_string())
    };

    let mut best: HashMap<String, (&String, usize)> = HashMap::new();
    for (g, n) in &variants {
        let cluster = resolve(&alnum_key(g));
        let score = |g: &str, n: usize| {
            n * 4
                + usize::from(g.contains(' ')) * 2
                + usize::from(g.chars().any(|c| c.is_uppercase()))
        };
        let entry = best.entry(cluster).or_insert((g, *n));
        if score(g, *n) > score(entry.0, entry.1) {
            *entry = (g, *n);
        }
    }

    let display: HashMap<String, String> = best
        .iter()
        .map(|(k, (g, _))| (k.clone(), (*g).clone()))
        .collect();
    for c in contributors.iter_mut() {
        if let Some(g) = &c.group {
            c.group = display
                .get(&resolve(&alnum_key(g)))
                .cloned()
                .or(c.group.clone());
        }
    }
    display.len()
}
