//! Build contributor timelines from a git or GitHub repository.
//!
//! `contributor-graphs` is primarily a command-line tool, but the same engine
//! is available as a library. The usual flow is [`analyze`] to turn a
//! repository into [`Contributor`] rows plus [`RepoMeta`], then one of the
//! renderers in [`svg`] or [`html`].
//!
//! ```no_run
//! use contributor_graphs::{analyze, Config, svg};
//!
//! let analysis = analyze("nf-core/rnaseq", &Config::default())?;
//! let rows: Vec<_> = analysis.contributors.iter().filter(|c| !c.bot).cloned().collect();
//! let opts = svg::SvgOptions {
//!     width: 1100.0,
//!     title: analysis.meta.name.clone(),
//!     subtitle: String::new(),
//!     footer_left: String::new(),
//!     footer_right: String::new(),
//!     accent: "#2f6feb".into(),
//!     group_mode: false,
//!     dark: false,
//! };
//! std::fs::write("rnaseq.svg", svg::render_svg(&rows, &opts))?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! The lower-level modules ([`repo`], [`identity`], [`github`]) are public too,
//! so callers can assemble a custom pipeline.

pub mod github;
pub mod html;
pub mod identity;
pub mod model;
pub mod repo;
pub mod svg;

use anyhow::{bail, Result};

pub use model::{Contributor, RepoMeta};

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

/// Number of distinct calendar months in which a contributor committed.
pub fn active_months(c: &Contributor) -> u32 {
    c.months.iter().filter(|&&v| v > 0).count() as u32
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

/// Resolve a repository (local path, `owner/repo` slug, or git URL) into
/// contributor data and metadata.
pub fn analyze(input: &str, cfg: &Config) -> Result<Analysis> {
    macro_rules! log {
        ($($arg:tt)*) => { if cfg.verbose { eprintln!($($arg)*); } };
    }

    let prepared = repo::prepare(input, cfg.branch.as_deref())?;
    log!(
        "→ repository: {} (branch {})",
        prepared.display_name,
        prepared.branch
    );

    let commits = repo::read_commits(
        &prepared,
        cfg.branch.as_deref(),
        cfg.since.as_deref(),
        cfg.until.as_deref(),
        cfg.no_merges,
    )?;
    if commits.is_empty() {
        bail!("no commits found");
    }
    log!(
        "→ {} commits from {} distinct author emails",
        model::thousands(commits.len() as u64),
        distinct_emails(&commits)
    );

    let mut clusters = identity::cluster_commits(&commits, cfg.merge_names);

    let client = github::GhClient::new(if cfg.use_github {
        github::find_token()
    } else {
        None
    });
    if cfg.use_github {
        if let Some(slug) = &prepared.slug {
            log!("→ enriching from GitHub ({slug})");
            github::enrich_clusters(&mut clusters, &commits, slug, &client, cfg.verbose);
            clusters = identity::merge_by_login(clusters);
            github::fetch_profiles(&mut clusters, &client, cfg.verbose);
            if !cfg.detect_affiliation {
                for cl in clusters.iter_mut() {
                    cl.affiliation = None;
                }
            }
        } else {
            log!("→ not a GitHub repo, skipping enrichment");
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
        github::embed_avatars(&mut contributors, &client, cfg.avatar_size, cfg.verbose);
    }

    let first = contributors.iter().map(|c| c.first).min().unwrap_or(0);
    let last = contributors.iter().map(|c| c.last).max().unwrap_or(0);
    let meta = RepoMeta {
        name: cfg
            .title
            .clone()
            .unwrap_or_else(|| prepared.display_name.clone()),
        url: prepared.url.clone(),
        slug: prepared.slug.clone(),
        branch: prepared.branch.clone(),
        first,
        last,
        total_commits: commits.len() as u64,
        total_contributors: contributors.iter().filter(|c| !c.bot).count(),
        generated: chrono::Utc::now().format("%Y-%m-%d").to_string(),
    };

    Ok(Analysis { contributors, meta })
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
pub fn canonicalize_groups(contributors: &mut [Contributor]) -> usize {
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
