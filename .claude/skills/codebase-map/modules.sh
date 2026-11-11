#!/usr/bin/env bash
# Print every Rust file under the given directories with the first line of its
# `//!` module doc, so an agent can pick the file to open without reading each
# one. The doc comments are the canonical file inventory; this only lists them.
#
#   .claude/skills/codebase-map/modules.sh src/update data/src/audio
#   .claude/skills/codebase-map/modules.sh            # whole workspace
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
[ $# -eq 0 ] && set -- src data/src nokkvi-ipc/src
find "$@" -name '*.rs' -not -path '*/tests/*' | sort | while read -r f; do
  doc=$(grep -m1 '^//!' "$f" | sed 's#^//! *##' | cut -c1-120 || true)
  printf '%s | %s\n' "$f" "${doc:-—}"
done
