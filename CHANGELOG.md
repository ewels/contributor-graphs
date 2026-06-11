# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/).

## [1.1.0] - 2026-06-11

### Added

- Accept multiple git sources in one run: commits are pooled into a single
  timeline, identities are resolved across all sources, and commits shared by
  overlapping histories are de-duplicated by SHA. New `analyze_many` library API.
  With several sources, a repo that fails to clone or has no commits is skipped
  with a warning instead of aborting the whole run (so org-wide views survive an
  empty or unreadable repo).
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
- Whole-org and multi-owner runs: a bare GitHub `owner` (org or user) expands
  to every non-fork repository it owns, and several owners can be combined in
  one run, with overlapping commits de-duplicated by SHA. A same-owner pool
  (one repo or a whole org) shows the owner's avatar in the interactive header
  and is titled by the owner.
- On-disk cache under `$XDG_CACHE_HOME/contributor-graphs` (bare clones, parsed
  git history keyed by the branch tip SHA, and GitHub author/profile/avatar
  lookups and org listings). A quick `git ls-remote` checks each repo's tip, so
  an unchanged repo skips the fetch, the log parse, and the API calls; a warm
  whole-org run drops from minutes to seconds. `--refresh` forces a fresh pull.
- Co-authored commits: `Co-authored-by` trailers are counted as commits for
  each co-author, with full credit (a commit counts for its author and each
  co-author), on by default. `--no-co-authors` disables it, and the interactive
  page has a live "Co-authors" toggle.

### Changed

- The interactive page's total-activity strip can line its plotting area up
  with the contributor rows, so a spike in overall activity sits directly above
  the rows that drove it. Toggle it with the ↔ button on the activity bar; the
  strip stays full-width by default.
- The affiliation row mode and its search are labelled "current affiliations",
  since the grouping comes from each contributor's present GitHub profile and
  is often out of date for past commits.
- The expanded-row detail panel is larger and uses a higher-contrast colour, so
  it stays readable (it was small and faint, especially in dark mode).

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

[1.1.0]: https://github.com/ewels/contributor-graphs/releases/tag/v1.1.0
[1.0.0]: https://github.com/ewels/contributor-graphs/releases/tag/v1.0.0
