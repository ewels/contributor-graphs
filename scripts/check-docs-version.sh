#!/usr/bin/env bash
#
# Keep the pinned install version in docs/index.html in sync with Cargo.toml.
#
# The "prebuilt binary" install snippet hard-codes VERSION=v<x.y.z> (in both the
# visible code and the copy-to-clipboard payload). Cargo.toml is the source of
# truth: the release tag is v<version>. This rewrites the docs to match and
# fails if it had to change anything, so a stale version can't be committed.
#
# Wired in as a local prek hook (always_run); see prek.toml. Run by hand with:
#   scripts/check-docs-version.sh
set -euo pipefail

cd "$(dirname "$0")/.."

DOC="docs/index.html"

VERSION="$(sed -n -E 's/^version = "([^"]+)".*/\1/p' Cargo.toml | head -1)"
if [ -z "$VERSION" ]; then
  echo "check-docs-version: could not read version from Cargo.toml" >&2
  exit 1
fi

before="$(cat "$DOC")"

# Two spots carry a v<x.y.z> tag: the VERSION= shell var (copy payload and
# visible token) and the highlighted token span. Scope the substitutions so we
# never touch an unrelated version string elsewhere in the page.
VERSION="$VERSION" perl -0pi -e '
  my $v = $ENV{VERSION};
  s/VERSION=v\d+\.\d+\.\d+/VERSION=v$v/g;
  s{(<span class="tk-num">)v\d+\.\d+\.\d+(</span>)}{$1v$v$2}g;
' "$DOC"

if [ "$before" != "$(cat "$DOC")" ]; then
  echo "check-docs-version: synced $DOC install version to v$VERSION (from Cargo.toml)." >&2
  echo "  Re-stage $DOC and commit again." >&2
  exit 1
fi
