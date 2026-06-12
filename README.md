<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/ewels/contributor-graphs/main/docs/logo-dark.svg">
    <img src="https://raw.githubusercontent.com/ewels/contributor-graphs/main/docs/logo.svg" alt="contributor-graphs" height="76">
  </picture>
</p>

<p align="center">
  Contributor timelines for any git or GitHub repository: a publication-ready
  SVG and a self-contained interactive HTML page.
</p>

<p align="center">
  <a href="https://github.com/ewels/contributor-graphs/actions/workflows/ci.yml"><img src="https://github.com/ewels/contributor-graphs/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/ewels/contributor-graphs/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License: Apache-2.0"></a>
  <a href="https://ewels.github.io/contributor-graphs/"><img src="https://img.shields.io/badge/docs-website-2f5fd0.svg" alt="Docs"></a>
</p>

The x-axis is time (first commit to today); each row is a contributor. Bars
are shaded by monthly commit activity, so it's easy to see who was active when
and how a project grew over the years.

<p align="center">
  <a href="https://ewels.github.io/contributor-graphs/">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/ewels/contributor-graphs/main/docs/app-rnaseq-dark.png">
      <img src="https://raw.githubusercontent.com/ewels/contributor-graphs/main/docs/app-rnaseq.png" alt="The interactive contributor-graphs page for nf-core/rnaseq" width="100%">
    </picture>
  </a>
</p>

<p align="center">
  <b><a href="https://ewels.github.io/contributor-graphs/">Documentation &amp; live examples →</a></b>
</p>

## Features

- 📊 **Two files per run:** a static SVG for embedding and a self-contained
  interactive HTML page, from a single command.
- 🎯 **Works with anything:** a local path (`.`), a GitHub slug
  (`nf-core/rnaseq`), any git URL, or a bare `owner` (org/user) that expands to
  all its repos.
- 🧩 **Many repos, one timeline:** pass as many repos and whole orgs as you
  like; their histories pool into a single chart, with commits shared across
  them deduplicated by SHA.
- 🔥 **Activity heat:** each bar is shaded by the contributor's monthly commit
  volume, so busy and quiet stretches read at a glance.
- 🐙 **GitHub enrichment:** resolves real names, `@usernames` and avatars via
  the GitHub API, using your `gh` CLI token automatically to avoid rate limits.
- 🔗 **Identity merging:** folds together the many name and email spellings a
  single person accumulates over the years, with a manual override for the
  stragglers.
- 🤝 **Co-authors:** counts `Co-authored-by` trailers as commits for each
  co-author (full credit), on by default, with a live toggle in the page.
- 🏢 **Affiliation grouping:** auto-detects organisations from GitHub profile
  companies (e.g. _SciLifeLab_, _Seqera_) and colours by them. Optionally
  **collapse the whole chart to one row per affiliation**. A curation file adds
  **time-bounded affiliations** (so a person's row is coloured by the org active
  at each point) and **group-name aliases**.
- 🏷️ **Release markers:** every git tag is drawn as a vertical line on the
  timeline, with a toggle to show or hide them.
- ⚡ **Fast re-runs:** clones, parsed history, and GitHub lookups are cached
  under `~/.cache`, so re-running an unchanged repo (or a whole org) takes
  seconds.
- 🧹 **Noise filters:** exclude bots, set a minimum-commit threshold, cap to the
  top _N_ contributors. In the HTML these are live controls.
- 🖱️ **Interactive HTML:** search, sort, filter by affiliation, switch between
  per-contributor and per-affiliation rows, drag-to-zoom the timeline, hover for
  detail + activity sparkline, light / dark / Wikipedia themes, and SVG/PNG
  export. Everything is embedded in one file; no server needed.
- 📦 **One binary:** a single Rust binary with no runtime to install.

## Install

Grab a prebuilt binary, install with Cargo, or use Docker. The
[GitHub CLI](https://cli.github.com) (`gh`) is optional but recommended for
enrichment without rate limits.

**Prebuilt binary:** download the archive for your platform from the
[releases page](https://github.com/ewels/contributor-graphs/releases), unpack
it, and put `contributor-graphs` on your `PATH`. No toolchain required.

**Cargo** (needs [Rust](https://rustup.rs)):

```bash
cargo install contributor-graphs
```

**Docker:** published to the GitHub Container Registry:

```bash
docker run --rm -v "$PWD:/work" -e GITHUB_TOKEN \
  ghcr.io/ewels/contributor-graphs nf-core/rnaseq
```

## Usage

```bash
# A GitHub repo by slug: clones history, enriches, writes two files
contributor-graphs nf-core/rnaseq

# A local checkout
contributor-graphs . -o docs/

# A full git URL
contributor-graphs https://github.com/MultiQC/MultiQC

# Several sources pooled into one timeline (any mix of slugs, paths, URLs)
contributor-graphs nf-core/rnaseq nf-core/sarek MultiQC/MultiQC --title "nf-core + MultiQC"

# A whole org (bare owner expands to all its non-fork repos), skipping one
contributor-graphs nextflow-io --exclude-repo nf-validation
```

This writes `<repo>.svg` and `<repo>.html` into the output directory.

### Multiple sources

Pass more than one source to pool every commit into a single timeline. Author
identities are resolved across the whole pool, so someone who appears in several
repositories shows up as one row. Commits that appear in more than one source
(overlapping histories — e.g. a repo and a fork, or a branch grafted onto a
rewrite) are de-duplicated by commit SHA; disjoint sources (separate repos for
an org-wide view) simply concatenate. Use `--title` to name the combined chart.

### Authentication

To enrich with usernames and avatars (and dodge GitHub's anonymous rate limit),
the tool reads a token from `$GITHUB_TOKEN` or `$GH_TOKEN`, falling back to
`gh auth token` if neither is set. Locally, just be logged in:

```bash
gh auth login
```

In CI, the `$GITHUB_TOKEN` that GitHub Actions injects is picked up
automatically, with no extra setup:

```yaml
- run: contributor-graphs ${{ github.repository }} -o site/
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

Pass `--no-github` to skip all network calls and render from git data alone.

## Common options

| Flag                                | Description                                                          |
| ----------------------------------- | -------------------------------------------------------------------- |
| `-o, --output-dir <DIR>`            | Where to write outputs (default: `.`)                                |
| `--title <TITLE>`                   | Override the chart title                                             |
| `-b, --branch <REF>`                | Which branch/ref to read (default: `HEAD`)                           |
| `--since <DATE>` / `--until <DATE>` | Restrict the commit window                                           |
| `--min-commits <N>`                 | Hide contributors below `N` commits in the SVG (default: 1)          |
| `--min-span-days <N>`               | Drop one-off/short-burst contributors (first-to-last span) from SVG  |
| `--max-contributors <N>`            | Cap SVG rows to the top `N` by commits (default: 40)                 |
| `--include-bots`                    | Keep bot accounts (excluded by default)                              |
| `--exclude <PATTERN>`               | Drop contributors matching a name/login (repeatable)                 |
| `--exclude-repo <REPO>`             | Skip a repo when expanding an org (`owner/repo` or name, repeatable) |
| `--by-affiliation`                  | Collapse each row to a whole affiliation, not one person             |
| `--unaffiliated-label <TEXT>`       | Bucket name for people with no affiliation (default: `Unaffiliated`) |
| `--sort <KEY>`                      | `first` · `last` · `commits` · `duration` · `name`                   |
| `--config <FILE.yml>`               | Curation file: identities, group aliases, affiliations (see below)   |
| `--affiliations <FILE>`             | CSV/TSV affiliations: `username, full name, affiliation, start, end` |
| `--no-affiliation`                  | Disable auto group detection from profiles                           |
| `--no-name-merge`                   | Don't merge identities that share an author name                     |
| `--no-co-authors`                   | Don't credit `Co-authored-by` trailers (author only)                 |
| `--refresh`                         | Ignore the cache and pull history + GitHub data fresh                |
| `--accent <HEX>`                    | Bar accent colour (default: `#2f6feb`)                               |
| `--theme <ID>`                      | Theme id: `auto`, `light`, `dark`, `wikipedia`, or a custom one      |
| `--themes <FILE.json>`              | Define extra themes / configure the page's theme menu                |
| `--lock-theme`                      | Hide the page's theme switcher and pin to one theme                  |
| `--width <PX>`                      | Static SVG width (default: 1100)                                     |
| `--format <svg\|html\|both>`        | Which outputs to write (default: both)                               |
| `--open`                            | Open the HTML in your browser when done                              |

Run `contributor-graphs --help` for the full list.

### Themes

The interactive page has a single Theme dropdown (top right) offering Light,
Dark, and **Wikipedia**; it opens on your OS light/dark preference unless you
pick one, and the choice is remembered per browser. The Wikipedia theme borrows
the look of Wikipedia's "band members over time" timelines: a Linux Libertine
heading over a plain sans-serif body, Wikipedia colours, square controls, and a
distinct solid bar per contributor instead of activity-heat shading.

`--theme <ID>` sets the SVG's look and the page's initial theme; `auto` (the
default) renders the SVG light and lets the page follow the viewer's OS.

#### Custom themes

Define your own themes in a JSON file and pass it with `--themes`. Each theme
inherits from `extends` (a built-in or another custom theme; default `light`)
and overrides only what it needs:

```json
{
  "default": "seqera",
  "available": ["seqera", "dark"],
  "lock": false,
  "themes": {
    "seqera": {
      "label": "Seqera",
      "extends": "light",
      "accent": "#0d6273",
      "bg": "#f4f8f8"
    }
  }
}
```

- `default` — the theme the page opens with (also settable with `--theme`).
- `available` — which themes appear in the menu, in order (default: all).
- `lock` (or `--lock-theme`) — hide the menu and pin to a single theme, so you
  can ship one custom look with no switching.

A theme may set any of: `label`, `extends`, `dark` (bool, for `color-scheme`
and avatar shading), `flat` (bool, solid band bars + sans-serif chart font),
`radius` (px), `font_sans`, `font_display`, and the colours `bg`, `card`,
`border`, `border_strong`, `text`, `muted`, `faint`, `accent`, `accent_soft`,
`grid_year`, `grid_month`, `track`, `ctx_area`, `ctx_line`. Custom themes work
in both the SVG (`--theme <id>`) and the interactive page.

### Grouping by affiliation

Affiliations are detected automatically from the `company` field of each
contributor's GitHub profile. Variant spellings are merged, so `seqeralabs`,
`Seqera Labs` and `Seqera` all count as one group. The most common groups get
distinct colours; the long tail shares a neutral grey, and bots are dropped.

To make the affiliations the _subject_ of the chart, pass `--by-affiliation`:
one bar per organisation, with every member's commits merged into it. People
with no detected affiliation are pooled into a single "Unaffiliated" row
(rename it with `--unaffiliated-label`). In the interactive HTML this is the
**Rows** dropdown, so you can flip between people and organisations live.

```bash
contributor-graphs nf-core/rnaseq --by-affiliation
```

### Curation file (`--config`)

For manual control, pass a YAML curation file with any of three sections:
**identities** (merge a person's names/emails/logins), **aliases** (group-name
variants that mean the same org), and **affiliations** (who was where, and
when). Manual values are authoritative: they're never renamed by the
auto-merge.

```yaml
# curation.yml
identities:
  - [Alexander Peltzer, apeltzer, a.peltzer@gmail.com, Alex Peltzer]
  - [Patrick Hüther, phue]

aliases:
  Seqera: [Seqera Labs, seqeralabs]
  SciLifeLab: [Science for Life Laboratory]

affiliations:
  ewels:
    - { group: SciLifeLab, until: "2022-05" }
    - { group: Seqera, since: "2022-05" }
  apeltzer:
    - { group: QBiC, until: "2020-01" }
    - { group: Boehringer Ingelheim, since: "2020-01" }
```

```bash
contributor-graphs nf-core/rnaseq --config curation.yml
```

Each `affiliations` matcher is a name, email, or login; repeat periods to give
one person several affiliations over time. Dates are `YYYY`, `YYYY-MM`, or
`YYYY-MM-DD` (quote anything with a dash); `until` is exclusive, and overlaps
resolve to the later `since`. A contributor's row is then drawn as one bar per
period, coloured by the organisation active at the time, and the
**by-affiliation** view splits their commits across those orgs by date.

### Affiliations table (`--affiliations`)

If you'd rather keep affiliations in a spreadsheet-friendly table, pass a CSV or
TSV file instead of (or alongside) `--config`. The delimiter (comma or tab) is
auto-detected. Columns are `username`, `full name`, `affiliation`, `start`,
`end`; repeat the username for several periods. `start` / `end` use the same
date formats and may be blank for open-ended (`end` is exclusive). The
`full name` is optional — blank for most people — but when set it is
**authoritative**, overriding the GitHub profile and commit-derived names. A
header row and `#` comment lines are ignored.

```csv
username,full name,affiliation,start,end
ewels,Phil Ewels,SciLifeLab,2014,2022-05
ewels,Phil Ewels,Seqera,2022-05
apeltzer,,QBiC,,2020-01
apeltzer,,Boehringer Ingelheim,2020-01
```

```bash
contributor-graphs nf-core/rnaseq --affiliations affiliations.csv
```

Aliases can only be expressed in the YAML; combine the two files when you need
both (`--config aliases.yml --affiliations affiliations.csv`).

## How it works

1. `git log` extracts every commit's author name, email, timestamp, and
   `Co-authored-by` trailers (honouring `.mailmap`).
2. Commits (authors and co-authors alike) are clustered into identities by
   shared email, then by shared author name.
3. For GitHub repos, each identity is resolved to a login + avatar (noreply
   emails offline; the rest via the commits API), then profiles are fetched for
   real names and companies. Clusters that resolve to the same login merge.
4. Per-contributor stats and per-month activity bins are computed, with
   affiliations applied (auto-detected, or curated per the `--config` file).
5. The SVG and HTML are rendered. Avatars are embedded as data URIs so both
   files are fully self-contained.

Clones, parsed history (keyed by the branch tip), and GitHub lookups are cached
under `$XDG_CACHE_HOME/contributor-graphs` (`~/.cache/...`), so re-runs are fast;
`--refresh` bypasses the cache.

## Releasing

Releases are cut by publishing a GitHub Release whose tag is the version
(e.g. `v0.1.0`). That triggers the workflows to build cross-platform binaries,
attach them to the release, build the multi-arch Docker image, and publish to
crates.io.

crates.io publishing uses [Trusted Publishing](https://crates.io/docs/trusted-publishing)
(OIDC, so no API token is stored in the repo). One-time setup:

1. Publish the first version manually (Trusted Publishing can't attach to a
   crate that doesn't exist yet): create a short-lived token at crates.io →
   Account Settings → API Tokens, then `cargo publish` (run `cargo publish
--dry-run` first). Revoke the token afterwards.
2. On crates.io → the crate → Settings → Trusted Publishing, add a GitHub
   publisher: owner `ewels`, repo `contributor-graphs`, workflow `release.yml`.
3. From then on, bump `version` in `Cargo.toml`, then publish a GitHub Release. The workflow mints a short-lived token via OIDC
   and publishes automatically.

## License

[Apache-2.0](https://github.com/ewels/contributor-graphs/blob/main/LICENSE)
