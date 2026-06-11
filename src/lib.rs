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

    let prepared: Vec<repo::PreparedRepo> = inputs
        .iter()
        .map(|input| repo::prepare(input, cfg.branch.as_deref()))
        .collect::<Result<_>>()?;
    let source_slugs: Vec<Option<String>> = prepared.iter().map(|p| p.slug.clone()).collect();
    for p in &prepared {
        log!("→ source: {} (branch {})", p.display_name, p.branch);
    }

    // Read every source, tagging each commit with its source index, then merge
    // the pools and drop commits whose SHA we have already seen.
    let mut commits: Vec<model::Commit> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut duplicates = 0u64;
    for (i, p) in prepared.iter().enumerate() {
        let src_commits = repo::read_commits(
            p,
            cfg.branch.as_deref(),
            cfg.since.as_deref(),
            cfg.until.as_deref(),
            cfg.no_merges,
        )?;
        for mut c in src_commits {
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
    if inputs.len() > 1 {
        log!(
            "→ {} commits from {} sources ({} duplicate commits dropped), {} distinct author emails",
            model::thousands(commits.len() as u64),
            inputs.len(),
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

    let client = github::GhClient::new(if cfg.use_github {
        github::find_token()
    } else {
        None
    });
    let any_slug = source_slugs.iter().any(|s| s.is_some());
    if cfg.use_github {
        if any_slug {
            log!("→ enriching from GitHub");
            github::enrich_clusters(&mut clusters, &commits, &source_slugs, &client, cfg.verbose);
            clusters = identity::merge_by_login(clusters);
            github::fetch_profiles(&mut clusters, &client, cfg.verbose);
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
        github::embed_avatars(&mut contributors, &client, cfg.avatar_size, cfg.verbose);
    }

    // A single source keeps its slug/url/branch; multiple sources collapse to a
    // combined label with no single canonical URL.
    let single = if prepared.len() == 1 {
        Some(&prepared[0])
    } else {
        None
    };

    // Owner/org avatar for the interactive page header (single GitHub source).
    let owner_avatar = if cfg.use_github && cfg.embed_avatars {
        single
            .and_then(|p| p.slug.as_deref())
            .and_then(|s| s.split('/').next())
            .and_then(|owner| github::fetch_avatar(&client, owner, 48))
    } else {
        None
    };

    let default_name = match single {
        Some(p) => p.display_name.clone(),
        None => combined_name(&prepared),
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
    };

    Ok(Analysis { contributors, meta })
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
