use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use contributor_graphs::{analyze_many, html, model, svg, Analysis, Config, Contributor, Sort};
use model::{format_month_year, thousands};
use std::path::PathBuf;

/// Generate contributor timeline graphs for a git/GitHub repository:
/// a static SVG and a self-contained interactive HTML page.
#[derive(Parser)]
#[command(version, about, arg_required_else_help = true)]
struct Args {
    /// One or more sources: a local path, a GitHub `owner/repo` slug, a git
    /// URL, or a bare GitHub `owner` (org or user), which expands to all of
    /// that owner's non-fork repositories. Multiple sources are pooled into a
    /// single timeline (duplicate commits shared across overlapping histories
    /// are dropped by SHA).
    #[arg(required = true, num_args = 1..)]
    repos: Vec<String>,

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

    /// YAML curation file with manual `identities`, group-name `aliases`, and
    /// time-bounded `affiliations`. See the docs for the schema.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Skip all GitHub API enrichment (usernames, avatars)
    #[arg(long)]
    no_github: bool,

    /// Don't auto-detect group affiliations from GitHub profile companies
    #[arg(long)]
    no_affiliation: bool,

    /// Don't merge identities that share the same author name
    #[arg(long)]
    no_name_merge: bool,

    /// Don't count `Co-authored-by` trailers (count only each commit's author)
    #[arg(long)]
    no_co_authors: bool,

    /// Keep avatars as remote URLs instead of embedding data URIs
    #[arg(long)]
    no_embed_avatars: bool,

    /// Ignore cached git history and GitHub lookups and pull everything fresh
    /// (the cache is still updated with the new results)
    #[arg(long)]
    refresh: bool,

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

    /// Theme id to render: `auto` (SVG light; HTML follows the OS), a built-in
    /// (`light`, `dark`, `wikipedia`), or a custom id from `--themes`. Sets the
    /// SVG look and the interactive page's initial theme.
    #[arg(long, default_value = "auto")]
    theme: String,

    /// JSON file defining extra themes and the page's theme menu (default,
    /// available list, lock). See the README for the format.
    #[arg(long)]
    themes: Option<PathBuf>,

    /// In the interactive page, hide the theme switcher and pin to one theme
    #[arg(long)]
    lock_theme: bool,

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

/// The YAML curation file: manual identities, group-name aliases, and
/// time-bounded affiliations. Every section is optional.
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct CurationConfig {
    /// Each entry is `[canonical name, alias, …]`.
    #[serde(default)]
    identities: Vec<Vec<String>>,
    /// `canonical group: [variant, …]`.
    #[serde(default)]
    aliases: std::collections::BTreeMap<String, Vec<String>>,
    /// `matcher: [{group, since?, until?}, …]`.
    #[serde(default)]
    affiliations: std::collections::BTreeMap<String, Vec<AffiliationPeriod>>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AffiliationPeriod {
    group: String,
    #[serde(default)]
    since: Option<serde_yaml::Value>,
    #[serde(default)]
    until: Option<serde_yaml::Value>,
}

/// Parse a date string into a unix timestamp. Accepts `YYYY`, `YYYY-MM`, or
/// `YYYY-MM-DD` (start of the period).
fn parse_date(s: &str) -> Result<i64> {
    use chrono::{NaiveDate, TimeZone, Utc};
    let bad = || anyhow::anyhow!("invalid date {s:?} (use YYYY, YYYY-MM, or YYYY-MM-DD)");
    let p: Vec<&str> = s.trim().split('-').collect();
    let year: i32 = p[0].parse().map_err(|_| bad())?;
    let month: u32 = p.get(1).map_or(Ok(1), |m| m.parse()).map_err(|_| bad())?;
    let day: u32 = p.get(2).map_or(Ok(1), |d| d.parse()).map_err(|_| bad())?;
    let dt = NaiveDate::from_ymd_opt(year, month, day)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .ok_or_else(bad)?;
    Ok(Utc.from_utc_datetime(&dt).timestamp())
}

/// A YAML date value, tolerating an unquoted year (`2022`, a number) or a
/// quoted/dash date (`"2022-05"`); `None`/null means open-ended.
fn date_value(v: &Option<serde_yaml::Value>) -> Result<Option<i64>> {
    let s = match v {
        None | Some(serde_yaml::Value::Null) => return Ok(None),
        Some(serde_yaml::Value::String(s)) => s.clone(),
        Some(serde_yaml::Value::Number(n)) => n.to_string(),
        Some(other) => anyhow::bail!("invalid date value: {other:?}"),
    };
    parse_date(&s).map(Some)
}

struct Curation {
    identities: Vec<Vec<String>>,
    groups: Vec<contributor_graphs::model::GroupRule>,
    group_aliases: Vec<(String, Vec<String>)>,
}

/// Load and validate the YAML curation file into the pieces `analyze_many` needs.
fn load_curation(path: &PathBuf) -> Result<Curation> {
    use contributor_graphs::model::GroupRule;
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let cfg: CurationConfig = serde_yaml::from_str(&text)
        .with_context(|| format!("invalid curation YAML in {}", path.display()))?;
    let mut groups = Vec::new();
    for (matcher, periods) in &cfg.affiliations {
        for p in periods {
            groups.push(GroupRule {
                matcher: matcher.clone(),
                group: p.group.clone(),
                since: date_value(&p.since)?,
                until: date_value(&p.until)?,
            });
        }
    }
    Ok(Curation {
        identities: cfg.identities,
        groups,
        group_aliases: cfg.aliases.into_iter().collect(),
    })
}

fn main() -> Result<()> {
    let args = Args::parse();
    let started = std::time::Instant::now();
    eprintln!("contributor-graphs");

    // The accent colour is written raw into SVG attributes; reject anything
    // that could break out of them.
    if args.accent.contains(['"', '<', '>', '&']) {
        anyhow::bail!("invalid --accent colour: {:?}", args.accent);
    }

    let curation = match &args.config {
        Some(path) => load_curation(path)?,
        None => Curation {
            identities: Vec::new(),
            groups: Vec::new(),
            group_aliases: Vec::new(),
        },
    };

    let cfg = Config {
        branch: args.branch.clone(),
        since: args.since.clone(),
        until: args.until.clone(),
        no_merges: args.no_merges,
        title: args.title.clone(),
        exclude: args.exclude.clone(),
        groups: curation.groups,
        group_aliases: curation.group_aliases,
        identities: curation.identities,
        use_github: !args.no_github,
        detect_affiliation: !args.no_affiliation,
        merge_names: !args.no_name_merge,
        count_coauthors: !args.no_co_authors,
        embed_avatars: !args.no_embed_avatars,
        avatar_size: 64,
        refresh: args.refresh,
        verbose: true,
    };

    // ---- themes ----
    let theme_set = match &args.themes {
        Some(path) => contributor_graphs::theme::load_config(path)?,
        None => contributor_graphs::theme::ThemeSet::default(),
    };
    // `--theme auto`: SVG uses the configured default (or light); the HTML page
    // follows the OS unless a default is configured. An explicit id sets both.
    let explicit_theme = args.theme != "auto";
    let svg_theme_id = if explicit_theme {
        args.theme.clone()
    } else {
        theme_set.default.clone().unwrap_or_else(|| "light".into())
    };
    let svg_theme = theme_set
        .get(&svg_theme_id)
        .cloned()
        .with_context(|| format!("unknown --theme '{svg_theme_id}'"))?;
    let html_default_theme = if explicit_theme {
        Some(args.theme.clone())
    } else {
        theme_set.default.clone()
    };

    let sources: Vec<&str> = args.repos.iter().map(String::as_str).collect();
    let Analysis { contributors, meta } = analyze_many(&sources, &cfg)?;

    std::fs::create_dir_all(&args.output_dir)?;
    let basename = args.basename.clone().unwrap_or_else(|| {
        contributor_graphs::repo::sanitize(meta.slug.as_deref().unwrap_or(&meta.name))
    });

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
        if rows.is_empty() {
            eprintln!("  warning: no contributors matched the filters; SVG will be empty");
        }
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
            footer_right: format!("{} · Generated by ewels/contributor-graphs", meta.generated),
            accent: args.accent.clone(),
            by_affiliation: args.by_affiliation,
            theme: svg_theme.clone(),
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
            custom_themes: theme_set.custom.clone(),
            theme_order: theme_set.order.clone(),
            default_theme: html_default_theme.clone(),
            lock_theme: args.lock_theme || theme_set.lock,
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
