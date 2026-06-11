# contributor-graphs

A Rust CLI that turns a git or GitHub repository into a contributor timeline:
a static SVG and a self-contained interactive HTML page. The x-axis is time,
each row is a contributor (or, with `--by-affiliation`, an organisation), and
bars are shaded by monthly commit activity.

## Build & check

```bash
cargo build --release          # binary at target/release/contributor-graphs
cargo run -- nf-core/rnaseq -o /tmp   # run against a repo
cargo fmt --all                # format
cargo clippy --all-targets -- -D warnings
prek run --all-files           # pre-commit hooks (fmt, clippy, prettier, hygiene)
```

There is no test suite; validate changes by generating against real repos
(`nf-core/rnaseq`, `MultiQC/MultiQC`, a local checkout) and viewing the output
in a browser.

## Layout

- `src/main.rs`: CLI args (clap), pipeline orchestration, group canonicalisation.
- `src/repo.rs`: input resolution (local path / `owner/repo` / git URL / bare
  `owner` org expansion), bare partial clone into the cache, `git ls-remote`
  freshness checks, `git log` parsing.
- `src/cache.rs`: the on-disk cache under `$XDG_CACHE_HOME/contributor-graphs`
  (`~/.cache/...`). Holds the bare clones, per-repo parsed `git log` keyed by the
  branch tip SHA, and the GitHub lookups (SHA→author, login→profile,
  avatar→data URI). Makes re-runs fast; `--refresh` bypasses it.
- `src/identity.rs`: clustering commit identities by email then name, manual
  identity/group files, bot detection, building `Contributor` rows.
- `src/github.rs`: token discovery (`GITHUB_TOKEN` → `GH_TOKEN` → `gh auth
token`), parallel login/avatar resolution and profile/company lookup, avatar
  embedding as data URIs, `company` normalisation.
- `src/model.rs`: `Contributor` / `RepoMeta`, month-bin helpers, and
  `aggregate_by_group` (the `--by-affiliation` collapse).
- `src/svg.rs`: static SVG renderer (layout, time ticks, activity shading,
  group colours, legend).
- `src/html.rs` + `src/assets/template.html`: the interactive page. The Rust
  side serialises data to JSON and substitutes it into the template; all the
  interactivity (filters, brush-zoom, tooltips, affiliation toggle, theming,
  export) lives in the template's inline `<script>`.
- `docs/`: the documentation website (`index.html`), served via GitHub Pages.
  `rnaseq*.html` / `example-rnaseq*.svg` are generated showcase assets;
  regenerate them with `scripts/regen-examples.sh` (needs a GitHub token) when
  the renderers change. The `regen-examples` GitHub Action runs that script on
  every push to `main` that touches the source and commits the result. The
  `app-rnaseq*.png` screenshots and `demo.mp4` are captured by hand, not by the
  script.

## Conventions

- Keep the SVG and HTML renderers visually in sync, since they share layout
  constants, the time-tick logic, and the activity-shading maths. A change to
  one usually needs the mirror change in the other.
- The HTML output must stay a single self-contained file (avatars inlined, no
  external requests), so it renders offline and on `gh-pages`.
- Run `prek run --all-files` before committing. `prek.toml` is the config
  (not `.pre-commit-config.yaml`); prettier formats `docs/index.html`, Markdown,
  CSS/JS, YAML, and JSON, but the generated showcase files are excluded.
- Apache-2.0 licensed.
