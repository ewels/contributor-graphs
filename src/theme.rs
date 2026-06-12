//! Visual themes for both renderers.
//!
//! A [`Theme`] is a complete look: page colours, fonts, corner radius, and a
//! `flat` flag that switches the chart to solid "band member" bars. Three are
//! built in ([`builtins`]); more can be supplied at generation time via a JSON
//! file (see [`load_config`]) and offered in the interactive page's theme menu.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

const SANS: &str = "-apple-system, 'Segoe UI', Inter, 'Helvetica Neue', Arial, sans-serif";

/// A fully-resolved theme. String colours are emitted verbatim into CSS/SVG.
#[derive(Clone, Debug)]
pub struct Theme {
    pub id: String,
    pub label: String,
    /// Dark background: drives `color-scheme`, shadow choice, avatar lightness.
    pub dark: bool,
    /// Solid per-row band bars + a sans-serif chart font (the Wikipedia look).
    pub flat: bool,
    /// Corner radius for cards/controls, in pixels.
    pub radius: u32,
    pub font_sans: String,
    pub font_display: String,
    pub bg: String,
    pub card: String,
    pub border: String,
    pub border_strong: String,
    pub text: String,
    pub muted: String,
    pub faint: String,
    pub accent: String,
    /// Optional explicit accent-soft; derived from `accent` when absent.
    pub accent_soft: Option<String>,
    pub grid_year: String,
    pub grid_month: String,
    pub track: String,
    /// Colour of the release marker verticals.
    pub release: String,
    pub ctx_area: String,
    pub ctx_line: String,
}

impl Theme {
    /// HSL lightness (%) for initials-fallback avatar discs.
    pub fn avatar_l(&self) -> u32 {
        if self.dark {
            48
        } else {
            62
        }
    }

    fn accent_soft(&self) -> String {
        self.accent_soft
            .clone()
            .unwrap_or_else(|| rgba(&self.accent, 0.12))
    }

    fn shadow(&self) -> &'static str {
        if self.flat {
            "none"
        } else if self.dark {
            "0 1px 2px rgba(0,0,0,.4)"
        } else {
            "0 1px 2px rgba(16,24,40,.04), 0 1px 6px rgba(16,24,40,.05)"
        }
    }

    fn shadow_lg(&self) -> &'static str {
        if self.dark {
            "0 10px 32px rgba(0,0,0,.55)"
        } else {
            "0 8px 28px rgba(16,24,40,.14)"
        }
    }

    fn dim(&self) -> &'static str {
        if self.dark {
            "rgba(120,132,148,.14)"
        } else {
            "rgba(120,132,148,.18)"
        }
    }

    /// CSS custom properties (plus `color-scheme`) for a `[data-theme]` block.
    pub fn css_vars(&self) -> Vec<(String, String)> {
        vec![
            ("--bg".into(), self.bg.clone()),
            ("--card".into(), self.card.clone()),
            ("--border".into(), self.border.clone()),
            ("--border-strong".into(), self.border_strong.clone()),
            ("--text".into(), self.text.clone()),
            ("--muted".into(), self.muted.clone()),
            ("--faint".into(), self.faint.clone()),
            ("--accent".into(), self.accent.clone()),
            ("--accent-soft".into(), self.accent_soft()),
            ("--shadow".into(), self.shadow().into()),
            ("--shadow-lg".into(), self.shadow_lg().into()),
            ("--grid-year".into(), self.grid_year.clone()),
            ("--grid-month".into(), self.grid_month.clone()),
            ("--track".into(), self.track.clone()),
            ("--release".into(), self.release.clone()),
            ("--radius".into(), format!("{}px", self.radius)),
            ("--font-sans".into(), self.font_sans.clone()),
            ("--font-display".into(), self.font_display.clone()),
            (
                "color-scheme".into(),
                if self.dark { "dark" } else { "light" }.into(),
            ),
        ]
    }

    /// The fields the chart's JS needs (mirrors the CSS where they overlap).
    pub fn chart_json(&self) -> serde_json::Value {
        serde_json::json!({
            "label": self.label,
            "text": self.text,
            "muted": self.muted,
            "faint": self.faint,
            "gridYear": self.grid_year,
            "gridMonth": self.grid_month,
            "track": self.track,
            "release": self.release,
            "card": self.card,
            "ctxArea": self.ctx_area,
            "ctxLine": self.ctx_line,
            "dim": self.dim(),
            "font": self.font_sans,
            "flat": self.flat,
        })
    }

    /// Full serialisation for the interactive page: CSS block + chart fields.
    pub fn to_json(&self) -> serde_json::Value {
        let css: serde_json::Map<String, serde_json::Value> = self
            .css_vars()
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect();
        serde_json::json!({
            "id": self.id,
            "label": self.label,
            "css": css,
            "chart": self.chart_json(),
        })
    }
}

/// Convert `#rrggbb` into `rgba(r,g,b,a)`; falls back to a neutral tint.
fn rgba(hex: &str, alpha: f64) -> String {
    let h = hex.trim_start_matches('#');
    if h.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&h[0..2], 16),
            u8::from_str_radix(&h[2..4], 16),
            u8::from_str_radix(&h[4..6], 16),
        ) {
            return format!("rgba({r},{g},{b},{alpha})");
        }
    }
    format!("rgba(120,132,148,{alpha})")
}

/// The three built-in themes, in menu order: Light, Dark, Wikipedia.
pub fn builtins() -> Vec<Theme> {
    vec![
        Theme {
            id: "light".into(),
            label: "Light".into(),
            dark: false,
            flat: false,
            radius: 12,
            font_sans: SANS.into(),
            font_display: SANS.into(),
            bg: "#f6f7f9".into(),
            card: "#ffffff".into(),
            border: "#e4e7ec".into(),
            border_strong: "#d4d9e0".into(),
            text: "#1c2530".into(),
            muted: "#5d6b7c".into(),
            faint: "#98a3b1".into(),
            accent: "#2f6feb".into(),
            accent_soft: Some("rgba(47,111,235,.1)".into()),
            grid_year: "#e2e6ec".into(),
            grid_month: "#eef1f5".into(),
            track: "#e8ebf0".into(),
            release: "#6b7a99".into(),
            ctx_area: "#c9d7f5".into(),
            ctx_line: "#7d9ce8".into(),
        },
        Theme {
            id: "dark".into(),
            label: "Dark".into(),
            dark: true,
            flat: false,
            radius: 12,
            font_sans: SANS.into(),
            font_display: SANS.into(),
            bg: "#0d1117".into(),
            card: "#151b23".into(),
            border: "#262d37".into(),
            border_strong: "#333c48".into(),
            text: "#e6ebf2".into(),
            muted: "#9aa7b6".into(),
            faint: "#5f6c7b".into(),
            accent: "#2f6feb".into(),
            accent_soft: Some("rgba(83,140,255,.13)".into()),
            grid_year: "#232b35".into(),
            grid_month: "#1a212a".into(),
            track: "#2a323d".into(),
            release: "#8893ad".into(),
            ctx_area: "#23344f".into(),
            ctx_line: "#4a6da8".into(),
        },
        Theme {
            id: "wikipedia".into(),
            label: "Wikipedia".into(),
            dark: false,
            flat: true,
            radius: 2,
            font_sans: "sans-serif".into(),
            font_display: "'Linux Libertine', Georgia, 'Times New Roman', serif".into(),
            bg: "#ffffff".into(),
            card: "#ffffff".into(),
            border: "#c8ccd1".into(),
            border_strong: "#a2a9b1".into(),
            text: "#202122".into(),
            muted: "#54595d".into(),
            faint: "#72777d".into(),
            accent: "#3366cc".into(),
            accent_soft: Some("rgba(51,102,204,.1)".into()),
            grid_year: "#c8ccd1".into(),
            grid_month: "#eaecf0".into(),
            track: "#e8e8e8".into(),
            release: "#000000".into(),
            ctx_area: "#cdd9f2".into(),
            ctx_line: "#5b81d4".into(),
        },
    ]
}

/// A custom theme as written in the JSON config. Every colour is optional;
/// anything omitted is inherited from `extends` (default: `light`).
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawTheme {
    label: Option<String>,
    extends: Option<String>,
    dark: Option<bool>,
    flat: Option<bool>,
    radius: Option<u32>,
    font_sans: Option<String>,
    font_display: Option<String>,
    bg: Option<String>,
    card: Option<String>,
    border: Option<String>,
    border_strong: Option<String>,
    text: Option<String>,
    muted: Option<String>,
    faint: Option<String>,
    accent: Option<String>,
    accent_soft: Option<String>,
    grid_year: Option<String>,
    grid_month: Option<String>,
    track: Option<String>,
    release: Option<String>,
    ctx_area: Option<String>,
    ctx_line: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    /// Initial theme id for the page (overridden by `--theme`).
    default: Option<String>,
    /// Theme ids to offer in the menu, in order. Defaults to every theme.
    available: Option<Vec<String>>,
    /// Hide the menu and pin the page to the default theme.
    lock: Option<bool>,
    themes: Option<HashMap<String, RawTheme>>,
}

/// Themes plus the page's menu configuration.
pub struct ThemeSet {
    /// Every theme (built-ins first, then custom), used to resolve `--theme`.
    pub all: Vec<Theme>,
    /// Just the custom themes, serialised into the page.
    pub custom: Vec<Theme>,
    /// Theme ids to show in the menu, in order.
    pub order: Vec<String>,
    /// Initial theme from the config (`--theme` still wins).
    pub default: Option<String>,
    /// Hide the switcher and pin to a single theme.
    pub lock: bool,
}

impl Default for ThemeSet {
    fn default() -> Self {
        let all = builtins();
        let order = all.iter().map(|t| t.id.clone()).collect();
        ThemeSet {
            all,
            custom: Vec::new(),
            order,
            default: None,
            lock: false,
        }
    }
}

impl ThemeSet {
    pub fn get(&self, id: &str) -> Option<&Theme> {
        self.all.iter().find(|t| t.id == id)
    }
}

fn resolve(id: &str, raw: &RawTheme, base: &Theme) -> Theme {
    Theme {
        id: id.to_string(),
        label: raw.label.clone().unwrap_or_else(|| id.to_string()),
        dark: raw.dark.unwrap_or(base.dark),
        flat: raw.flat.unwrap_or(base.flat),
        radius: raw.radius.unwrap_or(base.radius),
        font_sans: raw
            .font_sans
            .clone()
            .unwrap_or_else(|| base.font_sans.clone()),
        font_display: raw
            .font_display
            .clone()
            .unwrap_or_else(|| base.font_display.clone()),
        bg: raw.bg.clone().unwrap_or_else(|| base.bg.clone()),
        card: raw.card.clone().unwrap_or_else(|| base.card.clone()),
        border: raw.border.clone().unwrap_or_else(|| base.border.clone()),
        border_strong: raw
            .border_strong
            .clone()
            .unwrap_or_else(|| base.border_strong.clone()),
        text: raw.text.clone().unwrap_or_else(|| base.text.clone()),
        muted: raw.muted.clone().unwrap_or_else(|| base.muted.clone()),
        faint: raw.faint.clone().unwrap_or_else(|| base.faint.clone()),
        accent: raw.accent.clone().unwrap_or_else(|| base.accent.clone()),
        // accent_soft is only inherited when the accent itself is inherited;
        // a new accent should derive a fresh tint.
        accent_soft: raw.accent_soft.clone().or_else(|| {
            if raw.accent.is_none() {
                base.accent_soft.clone()
            } else {
                None
            }
        }),
        grid_year: raw
            .grid_year
            .clone()
            .unwrap_or_else(|| base.grid_year.clone()),
        grid_month: raw
            .grid_month
            .clone()
            .unwrap_or_else(|| base.grid_month.clone()),
        track: raw.track.clone().unwrap_or_else(|| base.track.clone()),
        release: raw.release.clone().unwrap_or_else(|| base.release.clone()),
        ctx_area: raw
            .ctx_area
            .clone()
            .unwrap_or_else(|| base.ctx_area.clone()),
        ctx_line: raw
            .ctx_line
            .clone()
            .unwrap_or_else(|| base.ctx_line.clone()),
    }
}

/// Load a theme config JSON file and combine it with the built-ins.
pub fn load_config(path: &Path) -> Result<ThemeSet> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let raw: RawConfig = serde_json::from_str(&text)
        .with_context(|| format!("invalid theme config {}", path.display()))?;

    let builtin = builtins();
    let mut all = builtin.clone();
    let mut custom = Vec::new();

    if let Some(themes) = &raw.themes {
        // Sort by id for deterministic output (HashMap order is random).
        let mut ids: Vec<&String> = themes.keys().collect();
        ids.sort();
        for id in ids {
            let rt = &themes[id];
            if builtin.iter().any(|t| &t.id == id) {
                bail!("theme id '{id}' shadows a built-in theme; pick another id");
            }
            let base = match &rt.extends {
                Some(e) => all
                    .iter()
                    .find(|t| &t.id == e)
                    .cloned()
                    .with_context(|| format!("theme '{id}' extends unknown theme '{e}'"))?,
                None => builtin[0].clone(), // light
            };
            let theme = resolve(id, rt, &base);
            all.push(theme.clone());
            custom.push(theme);
        }
    }

    let order = match &raw.available {
        Some(list) => {
            for id in list {
                if !all.iter().any(|t| &t.id == id) {
                    bail!("'available' lists unknown theme '{id}'");
                }
            }
            list.clone()
        }
        None => all.iter().map(|t| t.id.clone()).collect(),
    };

    if let Some(d) = &raw.default {
        if !all.iter().any(|t| &t.id == d) {
            bail!("'default' theme '{d}' is not defined");
        }
    }

    Ok(ThemeSet {
        all,
        custom,
        order,
        default: raw.default.clone(),
        lock: raw.lock.unwrap_or(false),
    })
}
