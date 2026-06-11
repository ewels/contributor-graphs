use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use contributor_graphs::{analyze, html, model, svg, Analysis, Config, Contributor, Sort};
use model::{format_month_year, thousands};
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

    /// Minimum span in days from a contributor's first to last commit, for the
    /// static SVG (drops one-off and short-burst contributors)
    #[arg(long, default_value_t = 0)]
    min_span_days: i64,

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

    /// Background theme for the static SVG
    #[arg(long, value_enum, default_value = "light")]
    theme: SvgTheme,

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

impl From<SortKey> for Sort {
    fn from(k: SortKey) -> Sort {
        match k {
            SortKey::First => Sort::First,
            SortKey::Last => Sort::Last,
            SortKey::Commits => Sort::Commits,
            SortKey::Duration => Sort::Duration,
            SortKey::Name => Sort::Name,
        }
    }
}

#[derive(Copy, Clone, PartialEq, ValueEnum)]
enum Format {
    Svg,
    Html,
    Both,
}

#[derive(Copy, Clone, PartialEq, ValueEnum)]
enum SvgTheme {
    Light,
    Dark,
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

    let groups = match &args.groups {
        Some(path) => read_tsv(path)?
            .into_iter()
            .filter(|r| r.len() >= 2)
            .map(|r| (r[0].clone(), r[1].clone()))
            .collect(),
        None => Vec::new(),
    };
    let identities = match &args.identities {
        Some(path) => read_tsv(path)?,
        None => Vec::new(),
    };

    let cfg = Config {
        branch: args.branch.clone(),
        since: args.since.clone(),
        until: args.until.clone(),
        no_merges: args.no_merges,
        title: args.title.clone(),
        exclude: args.exclude.clone(),
        groups,
        identities,
        use_github: !args.no_github,
        detect_affiliation: !args.no_affiliation,
        merge_names: !args.no_name_merge,
        embed_avatars: !args.no_embed_avatars,
        avatar_size: 64,
        verbose: true,
    };

    let Analysis { contributors, meta } = analyze(&args.repo, &cfg)?;

    std::fs::create_dir_all(&args.output_dir)?;
    let basename = args
        .basename
        .clone()
        .unwrap_or_else(|| contributor_graphs::repo::sanitize(&meta.name));

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
        let min_span = args.min_span_days * 86400;
        rows.retain(|c| c.commits >= args.min_commits && (c.last - c.first) >= min_span);
        let eligible = rows.len();
        if rows.len() > args.max_contributors {
            rows.sort_by_key(|c| std::cmp::Reverse(c.commits));
            rows.truncate(args.max_contributors);
        }
        contributor_graphs::sort(&mut rows, args.sort.into());

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
            dark: args.theme == SvgTheme::Dark,
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
        contributor_graphs::sort(&mut all, Sort::First);
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
