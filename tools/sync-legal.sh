#!/usr/bin/env bash
# Copy the workspace LICENSE and NOTICE into every crate folder (and the Python wheel folder), because cargo only
# packages files inside a crate's own folder. Run it after adding a crate or editing either file.
#   tools/sync-legal.sh          copy
#   tools/sync-legal.sh --check  fail if any crate's copy is missing or differs (no changes)
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
check=0; [ "${1:-}" = "--check" ] && check=1
bad=0
for m in $(grep -E '^members *= *\[' "$root/Cargo.toml" | grep -oE '"[^"]+"' | tr -d '"'); do
  for f in LICENSE NOTICE; do
    if [ "$check" = 1 ]; then
      cmp -s "$root/$f" "$root/$m/$f" || { echo "out of date: $m/$f"; bad=1; }
    else
      cp "$root/$f" "$root/$m/$f"
    fi
  done
done
[ "$check" = 1 ] && [ "$bad" = 0 ] && echo "ok: LICENSE and NOTICE match in every crate"
exit "$bad"
