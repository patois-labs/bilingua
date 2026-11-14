#!/usr/bin/env bash
# Set the release version of every Bilingua crate in one step, and the matching `version =` requirement on every
# path dependency in the root Cargo.toml (crates.io refuses a path dependency without one).
#
# Usage (run from anywhere; edits the workspace this script sits in):
#   tools/set-version.sh 1.0.0              one version for every crate (all inherit [workspace.package] version)
#   tools/set-version.sh 1.0.0 0.10.0       split: the front door (bilingua, bilingua-model, bilingua-seg,
#                                           bilingua-cli, and the bilingua-py wheel) at 1.0.0, every other crate
#                                           at 0.10.0 (format crates, bilingua-zip, the test-only oracle)
#   tools/set-version.sh --caret 1.0.0 ...  write caret requirements ("1.0.0") instead of exact pins ("=1.0.0")
#   tools/set-version.sh --check            print each crate's version and fail if a path dependency has no
#                                           version or its requirement does not match the crate's version
#
# FRONT="bilingua bilingua-model" overrides the front-door list in split mode.
# Exact pins are the default: all crates are released together, and a document's parser stamp records the
# crate versions that wrote it. The script is idempotent and switching between the two modes is safe.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="$root/Cargo.toml"
FRONT="${FRONT:-bilingua bilingua-model bilingua-seg bilingua-cli bilingua-py}"

pin="="
check=0
args=()
for a in "$@"; do
  case "$a" in
    --caret) pin="" ;;
    --check) check=1 ;;
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
    *) args+=("$a") ;;
  esac
done

semver='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'

# Member folders, from the one-line `members = [...]` in the root manifest.
members() {
  grep -E '^members *= *\[' "$manifest" | grep -oE '"[^"]+"' | tr -d '"'
}

# The package name in a member manifest.
pkg_name() {
  grep -m1 -E '^name *= *"' "$root/$1/Cargo.toml" | sed -E 's/^name *= *"([^"]+)".*/\1/'
}

is_front() {
  local n="$1" f
  for f in $FRONT; do [ "$f" = "$n" ] && return 0; done
  return 1
}

ws_version() {
  awk '/^\[workspace.package\]/{s=1;next} /^\[/{s=0} s && /^version *=/{gsub(/.*= *"|".*/,"");print;exit}' "$manifest"
}

# A member's own version: its `version = "x"` line, or the workspace version when it inherits.
crate_version() {
  local m="$1" line
  line="$(grep -m1 -E '^version' "$root/$m/Cargo.toml")"
  case "$line" in
    version.workspace*) ws_version ;;
    *) echo "$line" | sed -E 's/^version *= *"([^"]+)".*/\1/' ;;
  esac
}

if [ "$check" = 1 ]; then
  bad=0
  declare -A ver
  for m in $(members); do n="$(pkg_name "$m")"; ver[$n]="$(crate_version "$m")"; echo "$n ${ver[$n]}"; done
  while IFS= read -r line; do
    dep="$(echo "$line" | sed -E 's/^([a-z0-9-]+) .*/\1/')"
    req="$(echo "$line" | grep -oE 'version = "[^"]*"' | sed -E 's/version = "=?([^"]*)"/\1/' || true)"
    if [ -z "$req" ]; then echo "MISSING version: $dep"; bad=1
    elif [ -n "${ver[$dep]:-}" ] && [ "$req" != "${ver[$dep]}" ]; then echo "MISMATCH: $dep requires $req, crate is ${ver[$dep]}"; bad=1; fi
  done < <(grep -E '^bilingua[a-z0-9-]* = \{ *path *=' "$manifest")
  [ "$bad" = 0 ] && echo "ok: every path dependency carries its crate's version"
  exit "$bad"
fi

if [ "${#args[@]}" -lt 1 ] || [ "${#args[@]}" -gt 2 ]; then sed -n '2,16p' "$0"; exit 2; fi
front="${args[0]}"
rest="${args[1]:-$front}"
for v in "$front" "$rest"; do
  [[ "$v" =~ $semver ]] || { echo "not a semver version: $v" >&2; exit 2; }
done

# 1. The workspace version (the front door, and every crate in single mode).
awk -v v="$front" '
  /^\[workspace.package\]/ {s=1; print; next}
  /^\[/ {s=0}
  s && /^version *=/ {print "version = \"" v "\""; next}
  {print}' "$manifest" > "$manifest.tmp" && mv "$manifest.tmp" "$manifest"

# 2. Each member: inherit when it shares the workspace version, else its own version line.
for m in $(members); do
  n="$(pkg_name "$m")"
  f="$root/$m/Cargo.toml"
  if [ "$rest" = "$front" ] || is_front "$n"; then
    sed -i -E '0,/^version/s/^version( *=.*|\.workspace *=.*)$/version.workspace = true/' "$f"
  else
    sed -i -E "0,/^version/s/^version( *=.*|\.workspace *=.*)\$/version = \"$rest\"/" "$f"
  fi
done

# 3. The requirement on every path dependency in [workspace.dependencies].
for m in $(members); do
  n="$(pkg_name "$m")"
  if [ "$rest" = "$front" ] || is_front "$n"; then v="$front"; else v="$rest"; fi
  # Replace an existing version requirement, or add one after the path.
  sed -i -E \
    -e "s/^($n = \{ *path *= *\"[^\"]*\"), *version *= *\"[^\"]*\"/\1, version = \"$pin$v\"/" \
    -e "/^$n = \{ *path *= *\"[^\"]*\" *\}\$/s/^($n = \{ *path *= *\"[^\"]*\") *\}\$/\1, version = \"$pin$v\" }/" \
    "$manifest"
done

"$0" --check >/dev/null || { "$0" --check; exit 1; }
if [ "$rest" = "$front" ]; then echo "set every crate to $front"; else echo "set the front door ($FRONT) to $front and every other crate to $rest"; fi
echo "next: cargo update --workspace (refreshes Cargo.lock), then update CHANGELOG.md"
