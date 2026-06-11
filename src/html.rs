use crate::model::{Contributor, RepoMeta};
use crate::theme::Theme;

const TEMPLATE: &str = include_str!("assets/template.html");

pub struct HtmlOptions {
    pub accent: String,
    pub by_affiliation: bool,
    pub unaffiliated_label: String,
    /// Custom themes to register in the page (built-ins live in the template).
    pub custom_themes: Vec<Theme>,
    /// Theme ids to offer in the menu, in order.
    pub theme_order: Vec<String>,
    /// Initial theme id; `None` follows the OS light/dark preference. A saved
    /// choice in `localStorage` still wins.
    pub default_theme: Option<String>,
    /// Hide the theme switcher and pin the page to one theme.
    pub lock_theme: bool,
}

impl Default for HtmlOptions {
    fn default() -> Self {
        HtmlOptions {
            accent: "#2f6feb".into(),
            by_affiliation: false,
            unaffiliated_label: "Unaffiliated".into(),
            custom_themes: Vec::new(),
            theme_order: vec!["light".into(), "dark".into(), "wikipedia".into()],
            default_theme: None,
            lock_theme: false,
        }
    }
}

pub fn render_html(meta: &RepoMeta, contributors: &[Contributor], opts: &HtmlOptions) -> String {
    let custom: Vec<serde_json::Value> = opts.custom_themes.iter().map(Theme::to_json).collect();
    let data = serde_json::json!({
        "repo": meta,
        "contributors": contributors,
        "accent": opts.accent,
        "byAffiliation": opts.by_affiliation,
        "unaffiliated": opts.unaffiliated_label,
        "themes": custom,
        "themeOrder": opts.theme_order,
        "defaultTheme": opts.default_theme,
        "lockTheme": opts.lock_theme,
    });
    // `<\/` keeps any `</script>` inside the JSON from terminating the tag.
    let json = serde_json::to_string(&data)
        .expect("serialize data")
        .replace("</", "<\\/");
    let title = format!("{} · contributors", meta.name);
    TEMPLATE
        .replace("__PAGE_TITLE__", &html_escape(&title))
        .replace("__DATA__", &json)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
