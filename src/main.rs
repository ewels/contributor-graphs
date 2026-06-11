mod github;
mod html;
mod identity;
mod model;
mod repo;
mod svg;

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use model::{format_month_year, thousands, Contributor, RepoMeta};
use std::path::PathBuf;

/// Generate contributor timeline graphs for a git/GitHub repository:
/// a static SVG and a self-contained interactive HTML page.
#[derive(Parser)]
#[command(version, about, arg_required_else_help = true)]
struct Args {
    /// Local path, GitHub `owner/repo` slug, or git URL
    repo: String,

    /// Directory to write outputs into
    #[arg(short, long, default_value = ".")]
    output_dir: PathBuf,

    /// Basename for output files (default: derived from repo name)
    #[arg(long)]
    basename: Option<String>,

    /// Chart title (default: repo name)
    #[arg(long)]
    title: Option<String>,

    /// Branch / ref to read history from (default: HEAD)
    #[arg(short, long)]
    branch: Option<String>,

    /// Only include commits after this date (passed to git, e.g. 2020-01-01)
    #[arg(long)]
    since: Option<String>,

    /// Only include commits before this date
    #[arg(long)]
    until: Option<String>,

    /// Skip merge commits
    #[arg(long)]
    no_merges: bool,

    /// Minimum commits for a contributor to appear in the static SVG
    #[arg(long, default_value_t = 1)]
    min_commits: u32,

    /// Maximum rows in the static SVG (top contributors by commits)
    #[arg(long, default_value_t = 40)]
    max_contributors: usize,

    /// Include bot accounts (excluded by default)
    #[arg(long)]
    include_bots: bool,

    /// Exclude contributors matching this name/email/login (repeatable)
    #[arg(long)]
    exclude: Vec<String>,

    /// TSV file mapping contributors to groups: `matcher<TAB>group`
    /// (matcher = name, email, or login)
    #[arg(long)]
    groups: Option<PathBuf>,

    /// TSV file merging identities: each row is `Canonical Name<TAB>alias…`
    #[arg(long)]
    identities: Option<PathBuf>,

    /// Skip all GitHub API enrichment (usernames, avatars)
    #[arg(long)]
    no_github: bool,

    /// Don't auto-detect group affiliations from GitHub profile companies
    #[arg(long)]
    no_affiliation: bool,

    /// Don't merge identities that share the same author name
    #[arg(long)]
    no_name_merge: bool,

    /// Keep avatars as remote URLs instead of embedding data URIs
    #[arg(long)]
    no_embed_avatars: bool,

    /// Width of the static SVG in pixels
    #[arg(long, default_value_t = 1100.0)]
    width: f64,

    /// Collapse each row to a whole affiliation instead of one person
    #[arg(long)]
    by_affiliation: bool,

    /// Label for contributors with no detected affiliation (in --by-affiliation)
    #[arg(long, default_value = "Unaffiliated")]
    unaffiliated_label: String,

    /// Row order in the static SVG
    #[arg(long, value_enum, default_value = "first")]
    sort: SortKey,

    /// Which outputs to generate
    #[arg(long, value_enum, default_value = "both")]
    format: Format,

    /// Accent colour for bars (hex)
    #[arg(long, default_value = "#2f6feb")]
    accent: String,

    /// Open the HTML output in a browser when done
    #[arg(long)]
    open: bool,
}

#[derive(Copy, Clone, PartialEq, ValueEnum)]
enum SortKey {
    /// First commit date (oldest contributors at the top)
    First,
    /// Most recent commit date
    Last,
    /// Total number of commits
    Commits,
    /// Length of active period
    Duration,
    /// Alphabetical
    Name,
}

#[derive(Copy, Clone, PartialEq, ValueEnum)]
enum Format {
    Svg,
    Html,
    Both,
}

fn read_tsv(path: &PathBuf) -> Result<Vec<Vec<String>>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').map(|f| f.trim().to_string()).collect())
        .collect())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let started = std::time::Instant::now();

    eprintln!("contributor-graphs");
    let prepared = repo::prepare(&args.repo, args.branch.as_deref())?;
    eprintln!(
        "→ repository: {} (branch {})",
        prepared.display_name, prepared.branch
    );

    let commits = repo::read_commits(
        &prepared,
        args.branch.as_deref(),
        args.since.as_deref(),
        args.until.as_deref(),
        args.no_merges,
    )?;
    if commits.is_empty() {
        bail!("no commits found");
    }
    let raw_identities = {
        let mut e: Vec<&str> = commits.iter().map(|c| c.email.as_str()).collect();
        e.sort_unstable();
        e.dedup();
        e.len()
    };
    eprintln!(
        "→ {} commits from {} distinct author emails",
        thousands(commits.len() as u64),
        raw_identities
    );

    // ---- identity clustering ----
    let mut clusters = identity::cluster_commits(&commits, !args.no_name_merge);

    // ---- GitHub enrichment ----
    let client = github::GhClient::new(if args.no_github {
        None
    } else {
        github::find_token()
    });
    if !args.no_github {
        if let Some(slug) = &prepared.slug {
            eprintln!("→ enriching from GitHub ({slug})");
            github::enrich_clusters(&mut clusters, &commits, slug, &client);
            clusters = identity::merge_by_login(clusters);
            github::fetch_profiles(&mut clusters, &client);
            if args.no_affiliation {
                for cl in clusters.iter_mut() {
                    cl.affiliation = None;
                }
            }
        } else {
            eprintln!("→ not a GitHub repo, skipping enrichment");
        }
    }

    if let Some(path) = &args.identities {
        let rows = read_tsv(path)?;
        clusters = identity::apply_identity_file(clusters, &rows);
        eprintln!("→ applied identity overrides from {}", path.display());
    }

    let groups: Vec<(String, String)> = match &args.groups {
        Some(path) => read_tsv(path)?
            .into_iter()
            .filter(|r| r.len() >= 2)
            .map(|r| (r[0].clone(), r[1].clone()))
            .collect(),
        None => Vec::new(),
    };

    let mut contributors = identity::build_contributors(&clusters, &commits, &groups);

    let n_groups = canonicalize_groups(&mut contributors);
    if n_groups > 0 {
        eprintln!("→ {n_groups} distinct affiliations/groups");
    }

    // Drop explicitly excluded contributors entirely.
    if !args.exclude.is_empty() {
        contributors.retain(|c| {
            !args.exclude.iter().any(|pat| {
                let p = pat.to_lowercase();
                c.name.to_lowercase().contains(&p)
                    || c.login
                        .as_deref()
                        .is_some_and(|l| l.to_lowercase().contains(&p))
            })
        });
    }

    let n_bots = contributors.iter().filter(|c| c.bot).count();
    eprintln!(
        "→ merged to {} contributors ({} bots)",
        contributors.len(),
        n_bots
    );

    // ---- avatars ----
    if !args.no_embed_avatars && !args.no_github {
        github::embed_avatars(&mut contributors, &client, 64);
    }

    // ---- metadata ----
    let first = contributors.iter().map(|c| c.first).min().unwrap_or(0);
    let last = contributors.iter().map(|c| c.last).max().unwrap_or(0);
    let meta = RepoMeta {
        name: args
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

    std::fs::create_dir_all(&args.output_dir)?;
    let basename = args
        .basename
        .clone()
        .unwrap_or_else(|| repo::sanitize(&prepared.display_name));

    // ---- static SVG ----
    if matches!(args.format, Format::Svg | Format::Both) {
        let base: Vec<Contributor> = contributors
            .iter()
            .filter(|c| args.include_bots || !c.bot)
            .cloned()
            .collect();
        let mut rows: Vec<Contributor> = if args.by_affiliation {
            model::aggregate_by_group(&base, &args.unaffiliated_label)
        } else {
            base
        };
        rows.retain(|c| c.commits >= args.min_commits);
        let eligible = rows.len();
        if rows.len() > args.max_contributors {
            rows.sort_by_key(|c| std::cmp::Reverse(c.commits));
            rows.truncate(args.max_contributors);
        }
        sort_rows(&mut rows, args.sort);

        let unit = if args.by_affiliation {
            "affiliations"
        } else {
            "contributors"
        };
        let mut notes = vec![
            if args.by_affiliation {
                format!("{} affiliations", eligible)
            } else {
                format!("{} contributors", meta.total_contributors)
            },
            format!("{} commits", thousands(meta.total_commits)),
            format!(
                "{} – {}",
                format_month_year(meta.first),
                format_month_year(meta.last)
            ),
        ];
        if rows.len() < eligible {
            notes.push(format!("showing top {} {unit} by commits", rows.len()));
        } else if args.min_commits > 1 {
            notes.push(format!("≥{} commits", args.min_commits));
        }

        let opts = svg::SvgOptions {
            width: args.width,
            title: meta.name.clone(),
            subtitle: notes.join("  ·  "),
            footer_left: meta
                .url
                .clone()
                .map(|u| u.trim_start_matches("https://").to_string())
                .unwrap_or_else(|| format!("branch {}", meta.branch)),
            footer_right: format!("generated {} · contributor-graphs", meta.generated),
            accent: args.accent.clone(),
            group_mode: args.by_affiliation,
        };
        let svg_str = svg::render_svg(&rows, &opts);
        let path = args.output_dir.join(format!("{basename}.svg"));
        std::fs::write(&path, &svg_str)?;
        eprintln!(
            "→ wrote {} ({} rows, {} KB)",
            path.display(),
            rows.len(),
            svg_str.len() / 1024
        );
    }

    // ---- interactive HTML ----
    if matches!(args.format, Format::Html | Format::Both) {
        let mut all = contributors.clone();
        sort_rows(&mut all, SortKey::First);
        let html_opts = html::HtmlOptions {
            accent: args.accent.clone(),
            by_affiliation: args.by_affiliation,
            unaffiliated_label: args.unaffiliated_label.clone(),
        };
        let html_str = html::render_html(&meta, &all, &html_opts);
        let path = args.output_dir.join(format!("{basename}.html"));
        std::fs::write(&path, &html_str)?;
        eprintln!(
            "→ wrote {} ({} contributors, {} KB)",
            path.display(),
            all.len(),
            html_str.len() / 1024
        );
        if args.open {
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open").arg(&path).status();
            #[cfg(not(target_os = "macos"))]
            let _ = std::process::Command::new("xdg-open").arg(&path).status();
        }
    }

    eprintln!("✓ done in {:.1}s", started.elapsed().as_secs_f64());
    Ok(())
}

/// Merge group-name variants that refer to the same organisation:
/// case/punctuation differences ("Seqera Labs" vs "seqeralabs") and
/// prefix forms ("Seqera" vs "Seqera Labs"). Returns the final group count.
fn canonicalize_groups(contributors: &mut [Contributor]) -> usize {
    use std::collections::HashMap;
    let alnum_key = |g: &str| -> String {
        let lower = g.to_lowercase();
        let trimmed = lower.strip_prefix("the ").unwrap_or(&lower);
        trimmed.chars().filter(|c| c.is_alphanumeric()).collect()
    };

    // Count members per raw variant.
    let mut variants: HashMap<String, usize> = HashMap::new();
    for c in contributors.iter() {
        if let Some(g) = &c.group {
            *variants.entry(g.clone()).or_default() += 1;
        }
    }

    // Map each variant to a cluster key, merging prefix forms (≥6 chars).
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

    // Pick the best display spelling per cluster: most members, then
    // prefer spellings with spaces and capital letters.
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

fn sort_rows(rows: &mut [Contributor], key: SortKey) {
    match key {
        SortKey::First => {
            rows.sort_by(|a, b| a.first.cmp(&b.first).then(b.commits.cmp(&a.commits)))
        }
        SortKey::Last => rows.sort_by(|a, b| b.last.cmp(&a.last).then(b.commits.cmp(&a.commits))),
        SortKey::Commits => rows.sort_by_key(|c| std::cmp::Reverse(c.commits)),
        SortKey::Duration => rows.sort_by_key(|c| std::cmp::Reverse(c.last - c.first)),
        SortKey::Name => rows.sort_by_key(|a| a.name.to_lowercase()),
    }
}
