#!/usr/bin/env bash
# Regenerate the format table in README.md from bilingua::filters() (the ids and extensions every registered filter
# reports). The table sits between the <!-- formats:start --> and <!-- formats:end --> lines.
#   tools/gen-format-table.sh          rewrite README.md
#   tools/gen-format-table.sh --check  fail if README.md is out of date (no changes)
# Needs cargo; it builds the facade and runs bilingua/examples/format_table.rs. A new filter id needs a name in
# that file first (the example says which).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
readme="$root/README.md"
table="$(cd "$root" && cargo run -q -p bilingua --example format_table)"
new="$(T="$table" awk '
  /<!-- formats:start -->/ {print; print ENVIRON["T"]; skip=1; next}
  /<!-- formats:end -->/ {skip=0}
  !skip {print}' "$readme")"
grep -q '<!-- formats:start -->' "$readme" || { echo "README.md has no <!-- formats:start --> marker" >&2; exit 2; }
if [ "${1:-}" = "--check" ]; then
  if [ "$new" = "$(cat "$readme")" ]; then echo "ok: README format table is current"; else echo "README format table is out of date: run tools/gen-format-table.sh" >&2; exit 1; fi
else
  printf '%s\n' "$new" > "$readme"
  echo "README.md format table: $(echo "$table" | tail -1)"
fi
