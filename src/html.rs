use crate::model::{Contributor, RepoMeta};

const TEMPLATE: &str = include_str!("assets/template.html");

pub struct HtmlOptions {
    pub accent: String,
    pub by_affiliation: bool,
    pub unaffiliated_label: String,
    /// Initial visual skin (`"default"` or `"wikipedia"`); a viewer can still
    /// switch it, and a saved choice in `localStorage` takes precedence.
    pub skin: String,
}

impl Default for HtmlOptions {
    fn default() -> Self {
        HtmlOptions {
            accent: "#2f6feb".into(),
            by_affiliation: false,
            unaffiliated_label: "Unaffiliated".into(),
            skin: "default".into(),
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
        // Initial theme the page opens with (a viewer can still switch, and a
        // saved choice wins). The "wikipedia" skin maps to the Wikipedia theme;
        // otherwise leave it to the OS light/dark preference.
        "theme": if opts.skin == "wikipedia" {
            serde_json::Value::String("wikipedia".into())
        } else {
            serde_json::Value::Null
        },
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
