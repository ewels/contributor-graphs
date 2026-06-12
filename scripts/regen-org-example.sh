#!/usr/bin/env bash
#
# Regenerate the whole-org showcase, docs/nf-core.{html,svg}, by pooling every
# non-fork repository in the nf-core GitHub org into one timeline.
#
# This clones ~180 repositories (history only) and enriches a few thousand
# contributors, so a cold run takes several minutes and a lot of GitHub API
# calls. The regen-examples CI workflow now runs it on a persistent cache (and
# with a higher API budget), so warm reruns are cheap; run it by hand (with
# `gh` logged in) when you want to refresh the org example locally.
set -euo pipefail

cd "$(dirname "$0")/.."

WIDTH="${WIDTH:-1180}"
BIN="target/release/contributor-graphs"

# Curation: group-name aliases live in the YAML, affiliations + display names in
# the TSV. Both are passed to every variant so the org example shows the curated
# affiliations rather than raw GitHub company strings.
CONFIG="${CONFIG:-scripts/examples-curation.yml}"
AFFILIATIONS="${AFFILIATIONS:-scripts/examples-affiliations.tsv}"
CURATE=(--config "$CONFIG" --affiliations "$AFFILIATIONS")

echo "==> building release binary"
cargo build --release --locked

# A bare owner name expands to all of that org's non-fork repositories inside
# the binary, so there is no separate `gh repo list` step any more.
echo "==> contributor-graphs nf-core (expands to every non-fork repo)"
"$BIN" nf-core "${CURATE[@]}" --basename nf-core \
  --format both --width "$WIDTH" -o docs

# The by-affiliation variant reuses the clones and GitHub data cached by the
# run above, so it is a fast warm rerun rather than a second full pull.
echo "==> contributor-graphs nf-core --by-affiliation"
"$BIN" nf-core "${CURATE[@]}" --by-affiliation --basename nf-core-affiliation \
  --format both --width "$WIDTH" -o docs

# Dark-theme figure SVGs so the docs site can swap card thumbnails to match its
# light/dark mode. Warm reruns off the cache above, so just the render differs.
echo "==> dark-theme SVG variants"
"$BIN" nf-core "${CURATE[@]}" --basename nf-core-dark \
  --format svg --width "$WIDTH" --theme dark -o docs
"$BIN" nf-core "${CURATE[@]}" --by-affiliation --basename nf-core-affiliation-dark \
  --format svg --width "$WIDTH" --theme dark -o docs

echo "==> done; wrote docs/nf-core{,-affiliation}{,-dark}.{html,svg}"
