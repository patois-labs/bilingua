#!/usr/bin/env bash
# Checks that the Bilingua parsing crates stay portable (browser WASM, desktop, server, Python):
#   1. cargo check for wasm32-unknown-unknown over every parsing crate;
#   2. no file system, thread, network, clock or process APIs in their non-test source.
# Run from anywhere. Exit 0 = portable.
set -u
cd "$(dirname "$0")/.."
CRATES="model zip seg text json markdown html po subtitle docx xlsx pptx xliff sdl ini strings toml properties resjson json5 jsmodule yaml arb resx xaml dtd wxl ftl qtts"
PKGS=""
for c in $CRATES; do PKGS="$PKGS -p bilingua-$c"; done
fail=0

if ! rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown; then
  echo "missing target: run  rustup target add wasm32-unknown-unknown"; exit 2
fi
echo "== cargo check --target wasm32-unknown-unknown =="
# shellcheck disable=SC2086
cargo check --target wasm32-unknown-unknown $PKGS -p bilingua || fail=1

echo "== forbidden APIs in parsing-crate source (tests excluded) =="
PATTERN='std::fs|std::thread|std::net|std::process|std::env|SystemTime|Instant::now|std::time::Instant|tokio|reqwest|rayon'
for c in $CRATES facade; do
  d="bilingua-$c"; [ "$c" = facade ] && d="bilingua"
  while IFS= read -r f; do
    case "$f" in */tests/*|*/tests.rs|*/tests_*.rs|*/benches/*) continue ;; esac
    # drop #[cfg(test)] items (inline test modules, `mod tests;`, test-only uses), then search
    hits=$(awk -f tools/non-test-lines.awk "$f" | grep -E "$PATTERN" | grep -vE '^[^:]+:[0-9]+: *//')
    if [ -n "$hits" ]; then echo "$hits"; fail=1; fi
  done < <(find "$d/src" -name '*.rs')
done

if [ $fail -eq 0 ]; then echo "portable: OK"; else echo "portable: FAILED"; fi
exit $fail
