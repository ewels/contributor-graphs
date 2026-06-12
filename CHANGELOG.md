# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/).

## [1.3.1] - 2026-06-13

### Added

- Progress bars for the slow phases — cloning repositories, reading history, and
  the GitHub enrichment passes (resolving identities, fetching profiles,
  embedding avatars) — so an org-wide run shows how many items remain instead of
  a line scrolling past per repo. Bars render only on a terminal; piped and CI
  output stays plain.
- `--exclude-repo`: when expanding a bare `owner` into all its repositories, skip
  one by `owner/repo` slug or bare repo name (case-insensitive, repeatable).

## [1.3.0] - 2026-06-13

### Added

- Release markers: vertical lines on the timeline at every git tag, with a
  "Releases" toggle in the filter controls. In light/dark themes they are off by
  default and, when switched on, replace the year/month gridlines so there is
  only ever one set of verticals; the Wikipedia theme always shows them (solid
  black) and never draws background gridlines. Tags are pooled from every source
  (multi-source tags are prefixed with the repo name), a short caption under the
  chart explains the lines, and they are included in the SVG and PNG exports.
- CSV/TSV affiliations file (`--affiliations`): columns `username`, `full name`,
  `affiliation`, `start`, `end`, one row per affiliation period. The delimiter
  is auto-detected and it combines with the `--config` YAML.

### Changed

- Refined Wikipedia theme: bright, near-CSS-primary colours for both the per-row
  "band member" bars and the affiliation groups; thicker grey row tracks with
  slim white gaps; solid black L-shaped x/y axes; x-axis tick marks; squared-off
  row hover; and no background gridlines.
- Smarter affiliation grouping: group-name variants ("Seqera Labs", "Seqera",
  "seqeralabs") are merged into one canonical organisation and a leading "The"
  is stripped, with identities backfilled across sources.
- The affiliation palette now cycles so every organisation gets a colour instead
  of the long tail collapsing into grey, the "Group" colour view is renamed
  "Affiliations", and legend chips filter the chart.
- Affiliation details are placed consistently across the tooltip and expanded
  rows throughout the report.

### Fixed

- Multi-affiliation contributors: the expanded-row area plot and the tooltip
  sparkline now colour the area fill per affiliation segment so it mirrors the
  line, instead of using a single colour.
- Gap months (no affiliation active that month) are coloured as unaffiliated
  grey rather than the row's primary group.
- The Wikipedia theme now shows the affiliation filter legend and colours rows
  by affiliation to match it when colouring by affiliation; the legend chips
  were previously hidden in that theme.

## [1.2.1] - 2026-06-12

### Changed

- Responsive interactive page: on narrow screens the filter controls and the
  organisation legend collapse into a slide-down drawer behind a hamburger
  button in a sticky navbar, instead of taking up most of the screen. The
  activity timeline stays out of the drawer and pins below the navbar so it is
  always visible while scrolling the chart. The desktop layout is unchanged.

## [1.2.0] - 2026-06-11

### Added

- Time-bounded affiliations: a contributor can hold several affiliations over
  time, each with a `since` / `until` date range (`until` is exclusive; dates
  are `YYYY`, `YYYY-MM`, or `YYYY-MM-DD`). On overlap the newer range wins and
  trims the older one. Each period renders as its own bar with rounded end caps,
  so a person who changed organisation shows a distinct bar per affiliation with
  gaps where there were no commits. The Wikipedia theme draws the same periods
  as separate solid bands.
- Co-authored commits: `Co-authored-by:` trailers are parsed from commit
  messages and each co-author receives full credit for the commit, folded into
  the same identity clustering as authors. On by default; disable with
  `--no-co-authors`, or toggle live in the interactive page.
- Unified YAML curation file (`--config`) replacing the old `--groups` /
  `--identities` TSV flags. One file holds `identities` (merge a person's
  several names/logins), `aliases` (group-name aliases, e.g. "Seqera Labs" and
  "Seqera" are the same organisation), and time-bounded `affiliations`.
- Caching for fast re-runs: git analysis is cached per repository and keyed by
  the branch tip SHA, with freshness checked via `git ls-remote`; GitHub login,
  profile, avatar, and org-listing lookups are cached with TTLs. Everything is
  stored under the XDG cache directory. `--refresh` bypasses the caches.
- Full-width toggle moved into the top button row (Full width | SVG | PNG |
  Dark), alongside the other controls.

### Changed

- Affiliation filtering now masks the bars and activity, not just which rows
  appear: selecting an affiliation shows only that affiliation's bar and its
  commit counts for each person.
- The tooltip and expanded row list all of a contributor's affiliations, not
  just the most recent one, and the legend's "+N more" is now a button that
  reveals every affiliation so any can be filtered.
- Affiliations are labelled "current affiliations", since auto-detected company
  data is often out of date.
- Expanded-row detail text is larger and higher contrast for readability.

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
