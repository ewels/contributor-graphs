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
BIN="target/release/contributor-graphs"

echo "==> building release binary"
cargo build --release --locked

gen() {
  echo "==> contributor-graphs $REPO $*"
  "$BIN" "$REPO" -o docs "$@"
}

# Default skin: the live interactive page and the static example SVG. They keep
# distinct basenames (rnaseq.html is the demo, example-rnaseq.svg the figure).
gen --basename rnaseq --format html
gen --basename example-rnaseq --format svg --width "$WIDTH"

# Wikipedia "band members over time" skin showcase.
gen --basename rnaseq-wikipedia --format html --skin wikipedia
gen --basename example-rnaseq-wikipedia --format svg --width "$WIDTH" --skin wikipedia

echo "==> done; regenerated SVG + HTML in docs/"
