#!/usr/bin/env bash
#
# Regenerate the whole-org showcase, docs/nf-core.{html,svg}, by pooling every
# non-fork repository in the nf-core GitHub org into one timeline.
#
# This clones ~180 repositories (history only) and enriches a few thousand
# contributors, so it takes several minutes and a lot of GitHub API calls — it
# is intentionally NOT part of regen-examples.sh or the CI auto-regeneration.
# Run it by hand (with `gh` logged in) when the org example needs refreshing.
set -euo pipefail

cd "$(dirname "$0")/.."

WIDTH="${WIDTH:-1180}"
BIN="target/release/contributor-graphs"

echo "==> building release binary"
cargo build --release --locked

# A bare owner name expands to all of that org's non-fork repositories inside
# the binary, so there is no separate `gh repo list` step any more.
echo "==> contributor-graphs nf-core (expands to every non-fork repo)"
"$BIN" nf-core --basename nf-core \
  --format both --width "$WIDTH" -o docs

# The by-affiliation variant reuses the clones and GitHub data cached by the
# run above, so it is a fast warm rerun rather than a second full pull.
echo "==> contributor-graphs nf-core --by-affiliation"
"$BIN" nf-core --by-affiliation --basename nf-core-affiliation \
  --format both --width "$WIDTH" -o docs

echo "==> done; wrote docs/nf-core{,-affiliation}.{html,svg}"
