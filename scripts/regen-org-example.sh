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

echo "==> listing non-fork nf-core repositories"
mapfile -t REPOS < <(
  gh repo list nf-core --limit 1000 --no-archived=false \
    --json nameWithOwner,isFork --jq '.[]|select(.isFork==false)|.nameWithOwner'
)
echo "    ${#REPOS[@]} repositories"

echo "==> contributor-graphs <${#REPOS[@]} sources> --title nf-core"
"$BIN" "${REPOS[@]}" --title "nf-core" --basename nf-core \
  --format both --width "$WIDTH" -o docs

echo "==> done; wrote docs/nf-core.html and docs/nf-core.svg"
