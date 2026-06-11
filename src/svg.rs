use crate::model::{format_month_year, month_start_ts, thousands, Contributor};
use crate::theme::Theme;
use chrono::{Datelike, TimeZone, Utc};
use std::collections::HashMap;
use std::fmt::Write as _;

pub struct SvgOptions {
    pub width: f64,
    pub title: String,
    pub subtitle: String,
    pub footer_left: String,
    pub footer_right: String,
    pub accent: String,
    /// Each row is a whole affiliation rather than one person.
    pub by_affiliation: bool,
    /// The theme to render in.
    pub theme: Theme,
}

impl Default for SvgOptions {
    fn default() -> Self {
        SvgOptions {
            width: 1100.0,
            title: String::new(),
            subtitle: String::new(),
            footer_left: String::new(),
            footer_right: String::new(),
            accent: "#2f6feb".into(),
            by_affiliation: false,
            theme: crate::theme::builtins().swap_remove(0), // light
        }
    }
}

pub const GROUP_PALETTE: &[&str] = &[
    "#4269d0", "#e7a13d", "#ff725c", "#6cc5b0", "#3ca951", "#ff8ab7", "#a463f2", "#97bbf5",
    "#9c6b4e", "#9498a0", "#2f7f8f", "#c65b8a",
];

const ROW_H: f64 = 26.0;
const BAR_H: f64 = 13.0;
const AVATAR: f64 = 18.0;

/// Flat, saturated, distinct per-row colours for the Wikipedia "band members"
/// look.
pub const BAND_PALETTE: &[&str] = &[
    "#2a64c4", "#d23a2e", "#2f9e44", "#e8910c", "#8a39b0", "#0e9aa7", "#d23a8e", "#6aa70e",
    "#3b4cc0", "#c0561e", "#1ba0c4", "#d6498b",
];

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn n(x: f64) -> String {
    if (x - x.round()).abs() < 0.005 {
        format!("{}", x.round() as i64)
    } else {
        format!("{x:.1}")
    }
}

/// Rough text width estimate for a 400-weight sans-serif, in px.
pub fn text_w(s: &str, size: f64) -> f64 {
    s.chars()
        .map(|c| match c {
            'i' | 'l' | 'j' | 'f' | 't' | 'r' | '.' | ',' | '\'' | '|' | ' ' | '(' | ')' => 0.32,
            'm' | 'w' | 'M' | 'W' | '@' => 0.88,
            c if c.is_uppercase() => 0.70,
            c if c.is_ascii_digit() => 0.56,
            _ => 0.54,
        })
        .sum::<f64>()
        * size
}

struct Ticks {
    major: Vec<(i64, String)>,
    minor: Vec<i64>,
}

fn year_ts(year: i32) -> i64 {
    Utc.with_ymd_and_hms(year, 1, 1, 0, 0, 0)
        .single()
        .map(|d| d.timestamp())
        .unwrap_or_default()
}

fn time_ticks(t0: i64, t1: i64) -> Ticks {
    let span_days = (t1 - t0) as f64 / 86400.0;
    let span_years = span_days / 365.25;
    let y0 = Utc
        .timestamp_opt(t0, 0)
        .single()
        .map(|d| d.year())
        .unwrap_or(1970);
    let y1 = Utc
        .timestamp_opt(t1, 0)
        .single()
        .map(|d| d.year())
        .unwrap_or(1970);

    let mut major = Vec::new();
    let mut minor = Vec::new();

    if span_years > 2.2 {
        let step = ((span_years / 11.0).ceil() as i32).max(1);
        for y in (y0..=y1 + 1).filter(|y| (*y - y0) % step == 0) {
            let ts = year_ts(y);
            if ts >= t0 && ts <= t1 {
                major.push((ts, y.to_string()));
            }
        }
        // Minor ticks: months for short spans, quarters / years for longer.
        let minor_months: i32 = if span_years <= 5.0 {
            1
        } else if span_years <= 11.0 {
            3
        } else {
            12
        };
        let m_start = (y0 - 1970) * 12;
        let m_end = (y1 + 1 - 1970) * 12 + 11;
        for m in (m_start..=m_end).filter(|m| m % minor_months == 0) {
            let ts = month_start_ts(m);
            if ts >= t0 && ts <= t1 && !major.iter().any(|(t, _)| *t == ts) {
                minor.push(ts);
            }
        }
    } else {
        // Short span: label months.
        let step = if span_days > 500.0 {
            3
        } else if span_days > 240.0 {
            2
        } else {
            1
        };
        let m_start = (y0 - 1970) * 12;
        let m_end = (y1 + 1 - 1970) * 12 + 11;
        let mut last_year_labelled = i32::MIN;
        for m in m_start..=m_end {
            let ts = month_start_ts(m);
            if ts < t0 || ts > t1 {
                continue;
            }
            if m % step == 0 {
                let year = 1970 + m.div_euclid(12);
                let mon = Utc
                    .timestamp_opt(ts, 0)
                    .single()
                    .map(|d| d.format("%b").to_string())
                    .unwrap_or_default();
                let label = if year != last_year_labelled {
                    last_year_labelled = year;
                    format!("{mon} {year}")
                } else {
                    mon
                };
                major.push((ts, label));
            } else {
                minor.push(ts);
            }
        }
    }
    Ticks { major, minor }
}

fn initials(name: &str) -> String {
    let mut it = name.split_whitespace().filter_map(|w| w.chars().next());
    let a = it.next().unwrap_or('?');
    match it.next_back() {
        Some(b) => format!("{a}{b}"),
        None => a.to_string(),
    }
    .to_uppercase()
}

fn hash_hue(name: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h % 360
}

pub const OTHER_GROUP_COLOR: &str = "#9aa3ad";
const MAX_GROUP_COLORS: usize = 10;

/// Assign palette colours to the most common groups; the long tail shares a
/// neutral grey. Returns (group → colour, legend entries in rank order).
pub fn group_colors(rows: &[Contributor]) -> (HashMap<String, String>, Vec<(String, String)>) {
    let mut counts_map: HashMap<String, usize> = HashMap::new();
    for c in rows {
        if let Some(g) = &c.group {
            *counts_map.entry(g.clone()).or_insert(0) += 1;
        }
        // Time-bounded affiliations contribute their other orgs too, so each
        // earns a colour and a legend entry even if it's nobody's current org.
        if let Some(mg) = &c.month_groups {
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for g in mg.iter().flatten() {
                if Some(g.as_str()) != c.group.as_deref() && seen.insert(g.as_str()) {
                    *counts_map.entry(g.clone()).or_insert(0) += 1;
                }
            }
        }
    }
    // Rank by frequency, breaking ties by name so the palette is deterministic.
    let mut counts: Vec<(String, usize)> = counts_map.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut map = HashMap::new();
    let mut legend = Vec::new();
    for (i, (g, _)) in counts.iter().enumerate() {
        if i < MAX_GROUP_COLORS {
            let color = GROUP_PALETTE[i % GROUP_PALETTE.len()].to_string();
            legend.push((g.clone(), color.clone()));
            map.insert(g.clone(), color);
        } else {
            map.insert(g.clone(), OTHER_GROUP_COLOR.to_string());
        }
    }
    if counts.len() > MAX_GROUP_COLORS {
        legend.push(("Other".into(), OTHER_GROUP_COLOR.into()));
    }
    (map, legend)
}

/// Lightly smooth monthly counts so density shading reads as a heat
/// gradient rather than a barcode.
pub fn smooth_months(months: &[u32]) -> Vec<f64> {
    let n = months.len();
    (0..n)
        .map(|i| {
            let prev = if i > 0 { months[i - 1] } else { 0 } as f64;
            let next = if i + 1 < n { months[i + 1] } else { 0 } as f64;
            (2.0 * months[i] as f64 + prev + next) / 4.0
        })
        .collect()
}

/// A contributor's bars: each contiguous affiliation period (when they have
/// time-bounded affiliations) or the single active span, as
/// `(group, first active month, last active month)` indices into `months`.
fn affiliation_runs(c: &Contributor) -> Vec<(Option<String>, usize, usize)> {
    let nm = c.months.len();
    let split = c.month_groups.is_some();
    let group_at = |k: usize| -> Option<String> {
        match &c.month_groups {
            Some(mg) => mg.get(k).and_then(|o| o.clone()),
            None => c.group.clone(),
        }
    };
    let mut runs = Vec::new();
    let mut i = 0;
    while i < nm {
        let g = group_at(i);
        let mut j = if split { i } else { nm.saturating_sub(1) };
        if split {
            while j + 1 < nm && group_at(j + 1) == g {
                j += 1;
            }
        }
        let (mut start, mut end) = (None, 0usize);
        for (k, &v) in c.months.iter().enumerate().take(j + 1).skip(i) {
            if v > 0 {
                start.get_or_insert(k);
                end = k;
            }
        }
        if let Some(a) = start {
            runs.push((g, a, end));
        }
        i = j + 1;
    }
    runs
}

pub fn render_svg(rows: &[Contributor], opts: &SvgOptions) -> String {
    let th = &opts.theme;
    let width = opts.width.max(360.0);
    // Colours are interpolated raw into SVG attributes; keep the user-supplied
    // accent from breaking out of them.
    let accent = esc(&opts.accent);

    let t_first = rows.iter().map(|c| c.first).min().unwrap_or(0);
    let t_last = rows.iter().map(|c| c.last).max().unwrap_or(1);
    let pad_t = (((t_last - t_first) as f64) * 0.012) as i64 + 86400;
    let (t0, t1) = (t_first - pad_t, t_last + pad_t);

    let (gcolors, mut legend) = group_colors(rows);
    let has_groups = !gcolors.is_empty();
    if has_groups
        && rows.iter().any(|c| c.group.is_none())
        && !legend.iter().any(|(g, _)| g == "Other")
    {
        legend.push(("Other".into(), OTHER_GROUP_COLOR.into()));
    }
    // In affiliation mode every row is its own group, so the legend would just
    // duplicate the y-axis — suppress it.
    let show_legend = has_groups && !opts.by_affiliation;
    if !show_legend {
        legend.clear();
    }

    // ---- layout ----
    let name_size = 12.5;
    let max_name_w = rows
        .iter()
        .map(|c| text_w(&c.name, name_size))
        .fold(0.0_f64, f64::max);
    let label_w = (max_name_w + AVATAR + 56.0).clamp(140.0, 380.0);
    let count_col = 64.0;
    let margin_r = 18.0;
    let chart_x = label_w;
    let chart_w = width - label_w - count_col - margin_r;

    let header_h = 74.0;
    // Lay out the legend with wrapping so every coloured group is shown.
    let mut legend_pos: Vec<(f64, usize, String, String)> = Vec::new();
    let legend_lines;
    {
        let mut x = chart_x;
        let mut line = 0usize;
        for (g, color) in &legend {
            let w = 15.0 + text_w(g, 11.0) + 22.0;
            if x + w > width - 20.0 && x > chart_x {
                line += 1;
                x = chart_x;
            }
            legend_pos.push((x, line, g.clone(), color.clone()));
            x += w;
        }
        legend_lines = if legend_pos.is_empty() { 0 } else { line + 1 };
    }
    let legend_h = if show_legend {
        legend_lines as f64 * 20.0 + 8.0
    } else {
        0.0
    };
    let chart_y = header_h + legend_h;
    let chart_h = rows.len() as f64 * ROW_H;
    let axis_h = 30.0;
    let footer_h = 26.0;
    let height = chart_y + chart_h + axis_h + footer_h;

    let sx = |ts: i64| chart_x + (ts - t0) as f64 / (t1 - t0) as f64 * chart_w;

    let mut s = String::with_capacity(256 * 1024);
    let font = th.font_sans.as_str();
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="{font}">"#,
        w = n(width),
        h = n(height)
    );
    let _ = write!(
        s,
        r#"<style>a{{text-decoration:none}}.row text{{transition:fill .1s}}.row:hover .bar-base{{opacity:.28}}</style>"#
    );
    let _ = write!(
        s,
        r#"<rect width="{w}" height="{h}" fill="{bg}"/>"#,
        w = n(width),
        h = n(height),
        bg = th.bg
    );

    // ---- header ----
    // Wikipedia article headings are serif and normal-weight, not bold.
    let title_weight = if th.flat { "400" } else { "700" };
    let _ = write!(
        s,
        r#"<text x="28" y="36" font-size="19" font-weight="{}" fill="{}" font-family="{}" letter-spacing="-0.2">{}</text>"#,
        title_weight,
        th.text,
        th.font_display,
        esc(&opts.title)
    );
    let _ = write!(
        s,
        r#"<text x="28" y="56" font-size="12" fill="{}">{}</text>"#,
        th.muted,
        esc(&opts.subtitle)
    );

    // ---- group legend ----
    for (x, line, g, color) in &legend_pos {
        let y = header_h - 4.0 + *line as f64 * 20.0;
        let _ = write!(
            s,
            r#"<rect x="{}" y="{}" width="10" height="10" rx="3" fill="{color}"/>"#,
            n(*x),
            n(y)
        );
        let _ = write!(
            s,
            r#"<text x="{}" y="{}" font-size="11" fill="{}">{}</text>"#,
            n(x + 15.0),
            n(y + 9.0),
            th.muted,
            esc(g)
        );
    }

    // ---- gridlines ----
    let ticks = time_ticks(t0, t1);
    let grid_top = chart_y - 6.0;
    let grid_bot = chart_y + chart_h + 6.0;
    for ts in &ticks.minor {
        let x = sx(*ts);
        let _ = write!(
            s,
            r#"<line x1="{x}" y1="{y1}" x2="{x}" y2="{y2}" stroke="{c}" stroke-width="1"/>"#,
            x = n(x),
            y1 = n(grid_top),
            y2 = n(grid_bot),
            c = th.grid_month
        );
    }
    for (ts, label) in &ticks.major {
        let x = sx(*ts);
        let _ = write!(
            s,
            r#"<line x1="{x}" y1="{y1}" x2="{x}" y2="{y2}" stroke="{c}" stroke-width="1"/>"#,
            x = n(x),
            y1 = n(grid_top),
            y2 = n(grid_bot),
            c = th.grid_year
        );
        let _ = write!(
            s,
            r#"<text x="{x}" y="{y}" font-size="11" font-weight="600" fill="{c}" text-anchor="middle">{label}</text>"#,
            x = n(x),
            y = n(grid_bot + 17.0),
            c = th.muted,
            label = esc(label)
        );
    }
    // Baseline under the chart.
    let _ = write!(
        s,
        r#"<line x1="{x1}" y1="{y}" x2="{x2}" y2="{y}" stroke="{c}" stroke-width="1"/>"#,
        x1 = n(chart_x - 4.0),
        x2 = n(chart_x + chart_w + 4.0),
        y = n(grid_bot),
        c = th.grid_year
    );

    // ---- defs: avatar clips ----
    let _ = write!(s, "<defs>");
    for (i, c) in rows.iter().enumerate() {
        let y = chart_y + i as f64 * ROW_H;
        if c.avatar.is_some() {
            let cy = y + ROW_H / 2.0;
            let cx = label_w - 16.0 - AVATAR / 2.0;
            let _ = write!(
                s,
                r#"<clipPath id="av{i}"><circle cx="{cx}" cy="{cy}" r="{r}"/></clipPath>"#,
                cx = n(cx),
                cy = n(cy),
                r = n(AVATAR / 2.0)
            );
        }
    }
    let _ = write!(s, "</defs>");

    // ---- rows ----
    for (i, c) in rows.iter().enumerate() {
        let y = chart_y + i as f64 * ROW_H;
        let cy = y + ROW_H / 2.0;
        let color = c
            .group
            .as_ref()
            .and_then(|g| gcolors.get(g).cloned())
            .unwrap_or_else(|| {
                if has_groups {
                    OTHER_GROUP_COLOR.to_string()
                } else if th.flat {
                    // Wikipedia skin without groups: a distinct band per row.
                    BAND_PALETTE[i % BAND_PALETTE.len()].to_string()
                } else {
                    accent.clone()
                }
            });

        let _ = write!(s, r#"<g class="row">"#);
        if opts.by_affiliation {
            let people = if c.members == 1 { "person" } else { "people" };
            let _ = write!(
                s,
                "<title>{} — {} {}, {} commits, {} – {}</title>",
                esc(&c.name),
                c.members,
                people,
                thousands(c.commits as u64),
                esc(&format_month_year(c.first)),
                esc(&format_month_year(c.last))
            );
        } else {
            let _ = write!(
                s,
                "<title>{} — {} commits, {} – {}</title>",
                esc(&c.name),
                thousands(c.commits as u64),
                esc(&format_month_year(c.first)),
                esc(&format_month_year(c.last))
            );
        }

        // Avatar, member-count badge (affiliation mode), or initials fallback.
        let acx = label_w - 16.0 - AVATAR / 2.0;
        if opts.by_affiliation {
            let label = if c.members < 100 {
                c.members.to_string()
            } else {
                "99+".into()
            };
            let fs = if c.members < 100 { 8.5 } else { 6.5 };
            let _ = write!(
                s,
                r##"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{color}"/><text x="{cx}" y="{ty}" font-size="{fs}" font-weight="700" fill="#fff" text-anchor="middle">{label}</text>"##,
                cx = n(acx),
                cy = n(cy),
                r = n(AVATAR / 2.0),
                ty = n(cy + 2.9),
                fs = n(fs),
                color = color,
                label = esc(&label)
            );
        } else {
            match &c.avatar {
                Some(href) => {
                    let _ = write!(
                        s,
                        r#"<image x="{x}" y="{y}" width="{d}" height="{d}" preserveAspectRatio="xMidYMid slice" clip-path="url(#av{i})" href="{href}"/>"#,
                        x = n(acx - AVATAR / 2.0),
                        y = n(cy - AVATAR / 2.0),
                        d = n(AVATAR),
                        href = esc(href)
                    );
                }
                None => {
                    let hue = hash_hue(&c.name);
                    let _ = write!(
                        s,
                        r##"<circle cx="{cx}" cy="{cy}" r="{r}" fill="hsl({hue},42%,{l}%)"/><text x="{cx}" y="{ty}" font-size="7.5" font-weight="700" fill="#fff" text-anchor="middle">{init}</text>"##,
                        cx = n(acx),
                        cy = n(cy),
                        r = n(AVATAR / 2.0),
                        ty = n(cy + 2.7),
                        hue = hue,
                        l = th.avatar_l(),
                        init = esc(&initials(&c.name))
                    );
                }
            }
        }

        // Name, right-aligned next to the avatar; linked when login known.
        let name_x = label_w - 16.0 - AVATAR - 8.0;
        let mut display = c.name.clone();
        while text_w(&display, name_size) > label_w - 52.0 && display.chars().count() > 4 {
            display = display.chars().take(display.chars().count() - 2).collect();
            display.push('…');
        }
        let name_text = format!(
            r#"<text x="{x}" y="{y}" font-size="{fs}" fill="{c}" text-anchor="end">{t}</text>"#,
            x = n(name_x),
            y = n(cy + 4.2),
            fs = n(name_size),
            c = th.text,
            t = esc(&display)
        );
        match &c.url {
            Some(u) => {
                let _ = write!(
                    s,
                    r#"<a href="{}" target="_blank">{}</a>"#,
                    esc(u),
                    name_text
                );
            }
            None => s.push_str(&name_text),
        }

        // Bar: a flat band per affiliation period (Wikipedia skin) or a faint
        // base span with monthly activity-heat segments (default skin). A change
        // of affiliation ends one bar and starts the next.
        let by = y + (ROW_H - BAR_H) / 2.0;
        let runs = affiliation_runs(c);
        let split = c.month_groups.is_some();
        let smoothed = (!th.flat).then(|| smooth_months(&c.months));
        let smax = smoothed.as_ref().map_or(1.0, |sm| {
            sm.iter().fold(0.0_f64, |a, &b| a.max(b)).max(1e-9)
        });
        for (bk, (g, a, end)) in runs.iter().enumerate() {
            let bar_color = if split {
                g.as_deref()
                    .and_then(|g| gcolors.get(g))
                    .map_or(color.as_str(), String::as_str)
            } else {
                color.as_str()
            };
            let x0 = sx(month_start_ts(c.m0 + *a as i32));
            let x1 = sx(month_start_ts(c.m0 + *end as i32 + 1));
            if th.flat {
                let w = (x1 - x0).max(6.0);
                let x = if x1 - x0 < 6.0 {
                    x0 + (x1 - x0) / 2.0 - 3.0
                } else {
                    x0
                };
                let _ = write!(
                    s,
                    r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="1.5" fill="{c}" opacity="0.92"/>"#,
                    x = n(x),
                    y = n(by),
                    w = n(w),
                    h = n(BAR_H),
                    c = bar_color,
                );
                continue;
            }
            let w = (x1 - x0).max(3.0);
            let rx = (BAR_H / 2.0).min(w / 2.0);
            let _ = write!(
                s,
                r#"<rect class="bar-base" x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" fill="{c}" opacity="0.16"/>"#,
                x = n(x0),
                y = n(by),
                w = n(w),
                h = n(BAR_H),
                r = n(rx),
                c = bar_color,
            );
            let sm = smoothed.as_ref().unwrap();
            if w > 6.0 {
                let _ = write!(
                    s,
                    r#"<clipPath id="bar{i}_{bk}"><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}"/></clipPath><g clip-path="url(#bar{i}_{bk})">"#,
                    x = n(x0),
                    y = n(by),
                    w = n(w),
                    h = n(BAR_H),
                    r = n(rx),
                );
                for (k, &sval) in sm.iter().enumerate().take(end + 1).skip(*a) {
                    if sval <= 0.0 {
                        continue;
                    }
                    let m = c.m0 + k as i32;
                    let mx0 = sx(month_start_ts(m));
                    let mx1 = sx(month_start_ts(m + 1));
                    let op = 0.28 + 0.72 * (sval / smax).sqrt();
                    let _ = write!(
                        s,
                        r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{c}" opacity="{op:.2}"/>"#,
                        x = n(mx0),
                        y = n(by),
                        w = n((mx1 - mx0).max(1.2)),
                        h = n(BAR_H),
                        c = bar_color,
                    );
                }
                let _ = write!(s, "</g>");
            } else {
                let _ = write!(
                    s,
                    r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{c}" opacity="0.9"/>"#,
                    cx = n(x0 + w / 2.0),
                    cy = n(cy),
                    r = n(BAR_H / 2.0 - 1.0),
                    c = bar_color,
                );
            }
        }

        // Commit count in the right margin.
        let _ = write!(
            s,
            r#"<text x="{x}" y="{y}" font-size="10.5" fill="{c}" text-anchor="end">{t}</text>"#,
            x = n(width - margin_r),
            y = n(cy + 3.8),
            c = th.faint,
            t = thousands(c.commits as u64)
        );
        let _ = write!(s, "</g>");
    }

    // ---- footer ----
    let fy = height - 9.0;
    let _ = write!(
        s,
        r#"<text x="28" y="{y}" font-size="10" fill="{c}">{t}</text>"#,
        y = n(fy),
        c = th.faint,
        t = esc(&opts.footer_left)
    );
    let _ = write!(
        s,
        r#"<text x="{x}" y="{y}" font-size="10" fill="{c}" text-anchor="end">{t}</text>"#,
        x = n(width - margin_r),
        y = n(fy),
        c = th.faint,
        t = esc(&opts.footer_right)
    );

    s.push_str("</svg>");
    s
}
