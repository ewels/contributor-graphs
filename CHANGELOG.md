# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- Accept multiple git sources in one run: commits are pooled into a single
  timeline, identities are resolved across all sources, and commits shared by
  overlapping histories are de-duplicated by SHA. New `analyze_many` library API.
- Expandable rows in the interactive page: click a row (or its chevron) to grow
  it into a detail card — the hover info (username, affiliation, commits, dates,
  active span) pinned on the left and a full-width monthly line plot on the
  right. The line plots share a fixed, mode-wide y-axis so expanded rows are
  directly comparable, and the summary bar is hidden while expanded. An
  "Expand all / Collapse all" control toggles every visible row at once.
- Theming system with a new **Wikipedia** theme, modelled on the EasyTimeline
  "band members over time" charts: a Linux Libertine heading over a plain
  sans-serif body, Wikipedia colours, square controls, and a distinct solid bar
  per contributor. The interactive page merges theme selection into a single
  top-nav dropdown (Light / Dark / Wikipedia) that defaults to the OS
  preference; the SVG takes `--theme wikipedia`.
- The interactive header shows the GitHub repository description, when it has one.
- Custom, extensible themes: define extra themes in a JSON file (`--themes`),
  inheriting from a built-in via `extends` and overriding only the colours/fonts
  you need. Configure which themes the page offers, the default, and whether the
  switcher is shown (`lock` / `--lock-theme`) — so you can ship a single custom
  look with no switching. `--theme <id>` selects the theme for both outputs and
  replaces the old `--theme light|dark` / `--skin` flags.

### Changed

- The interactive page's total-activity strip can line its plotting area up
  with the contributor rows, so a spike in overall activity sits directly above
  the rows that drove it. Toggle it with the ↔ button on the activity bar; the
  strip stays full-width by default.

## [1.0.0] - 2026-06-11

First public release.

- Turn any local git repo, GitHub `owner/repo` slug, or git URL into a
  contributor timeline: a publication-ready SVG and a self-contained
  interactive HTML page.
- GitHub enrichment for real names, usernames, and avatars, using the `gh`
  token or `GITHUB_TOKEN`/`GH_TOKEN`.
- Identity merging across name/email spellings, with manual override files.
- Affiliation grouping from profile companies, including a per-organisation
  view (`--by-affiliation`).
- Noise filters (bots, minimum commits, active span, top N) and, in the HTML,
  live search, sorting, group filtering, timeline zoom, and dark mode.
- Usable as a Rust library via the `analyze` API.

[1.0.0]: https://github.com/ewels/contributor-graphs/releases/tag/v1.0.0
