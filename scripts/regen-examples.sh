#!/usr/bin/env bash
#
# Regenerate the docs/ showcase SVG + HTML from nf-core/rnaseq.
#
# Run from anywhere inside the repo. Needs a GitHub token (GITHUB_TOKEN /
# GH_TOKEN, or `gh auth token`) so usernames and avatars resolve. The PNG
# screenshots (docs/app-rnaseq*.png) and demo.mp4 are NOT produced here —
# those are captured separately.
#
# Run it by hand after changing the renderers, or let the regen-examples
# GitHub Action run it on every push to main that touches the source.
set -euo pipefail

cd "$(dirname "$0")/.."

REPO="${REPO:-nf-core/rnaseq}"
WIDTH="${WIDTH:-1180}"
# Curation: group-name aliases live in the YAML, affiliations + display names in
# the TSV. Both are passed to every example so the demos show real affiliations.
CONFIG="${CONFIG:-scripts/examples-curation.yml}"
AFFILIATIONS="${AFFILIATIONS:-scripts/examples-affiliations.tsv}"
CURATE=(--config "$CONFIG" --affiliations "$AFFILIATIONS")
BIN="target/release/contributor-graphs"

echo "==> building release binary"
cargo build --release --locked

gen() {
  echo "==> contributor-graphs $REPO $*"
  "$BIN" "$REPO" "${CURATE[@]}" -o docs "$@"
}

# Default skin: the live interactive page and the static example SVG. They keep
# distinct basenames (rnaseq.html is the demo, example-rnaseq.svg the figure).
# Each figure SVG is also rendered with the dark theme so the docs site can
# swap the card thumbnail to match its light/dark mode.
gen --basename rnaseq --format html
gen --basename example-rnaseq --format svg --width "$WIDTH"
gen --basename example-rnaseq-dark --format svg --width "$WIDTH" --theme dark

# Wikipedia "band members over time" theme showcase (light-only by design).
gen --basename rnaseq-wikipedia --format html --theme wikipedia
gen --basename example-rnaseq-wikipedia --format svg --width "$WIDTH" --theme wikipedia

# Multi-source example: nf-core/sarek lost its pre-migration history in the move
# from SciLifeLab, so combining the two repos recovers the full timeline.
echo "==> contributor-graphs nf-core/sarek SciLifeLab/Sarek"
"$BIN" nf-core/sarek SciLifeLab/Sarek "${CURATE[@]}" --title "Sarek" \
  --basename sarek --format both --width "$WIDTH" -o docs
"$BIN" nf-core/sarek SciLifeLab/Sarek "${CURATE[@]}" --title "Sarek" \
  --basename sarek-dark --format svg --width "$WIDTH" --theme dark -o docs

echo "==> done; regenerated SVG + HTML in docs/"
echo "    (the whole-org docs/nf-core.* example is heavy; regenerate it with"
echo "     scripts/regen-org-example.sh when needed — it is not run in CI.)"
