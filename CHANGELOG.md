# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/).

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
