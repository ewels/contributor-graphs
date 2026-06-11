use crate::model::{Contributor, RepoMeta};

const TEMPLATE: &str = include_str!("assets/template.html");

pub struct HtmlOptions {
    pub accent: String,
    pub by_affiliation: bool,
    pub unaffiliated_label: String,
}

impl Default for HtmlOptions {
    fn default() -> Self {
        HtmlOptions {
            accent: "#2f6feb".into(),
            by_affiliation: false,
            unaffiliated_label: "Unaffiliated".into(),
        }
    }
}

pub fn render_html(meta: &RepoMeta, contributors: &[Contributor], opts: &HtmlOptions) -> String {
    let data = serde_json::json!({
        "repo": meta,
        "contributors": contributors,
        "accent": opts.accent,
        "byAffiliation": opts.by_affiliation,
        "unaffiliated": opts.unaffiliated_label,
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
