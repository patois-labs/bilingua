#!/usr/bin/env bash
# Publish every Bilingua crate to crates.io, one at a time, in dependency order. Resumable: a crate already on
# crates.io at this version is skipped, so after a stop (network, rate limit, a failed crate) just run it again.
#
#   tools/publish-crates.sh --dry-run     package and verify everything, upload nothing (cargo publish --dry-run
#                                         --workspace, which resolves the unpublished crates locally)
#   tools/publish-crates.sh --list        print the publish order and which crates are already up
#   tools/publish-crates.sh               the real publish (needs CARGO_REGISTRY_TOKEN in the environment)
#
# Options: --log FILE (default ~/bili/publish-<version>.log), --from CRATE (start at that crate; the skip check
# makes this rarely needed).
# A crate already up at this version is skipped only when crates.io lists one of our accounts as its owner: the
# owner logins come from BILINGUA_CRATES_OWNERS (space-separated crates.io logins) or, when that is unset, from the
# account of CARGO_REGISTRY_TOKEN. A crate up at this version under another owner (a squatted name), or one whose
# owners crates.io could not tell, stops the run, so a release never counts someone else's crate as ours.
# Every step goes to the log with a time stamp. Rate limits (HTTP 429) are waited out: the script sleeps until the
# time crates.io names, or 10 minutes when it names none, and tries the same crate again. Any other failure is
# retried 3 times (1, 2 then 4 minutes apart), then the script stops with exit code 1 and names the crate.
# The token is read only from the environment and never written or printed.
set -uo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
mode=publish; from=""; log=""
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) mode=dry ;;
    --list) mode=list ;;
    --from) from="$2"; shift ;;
    --log) log="$2"; shift ;;
    -h|--help) sed -n '2,19p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

version="$(awk '/^\[workspace.package\]/{s=1;next} /^\[/{s=0} s && /^version *=/{gsub(/.*= *"|".*/,"");print;exit}' Cargo.toml)"
log="${log:-$HOME/bili/publish-$version.log}"
mkdir -p "$(dirname "$log")"
UA="bilingua-release-script (brent@patoislabs.com)"

say() { printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" | tee -a "$log"; }

# 200 when <crate> <version> is on crates.io, 404 when not, anything else when crates.io could not be asked.
published() {
  curl -s -o /dev/null -w '%{http_code}' -A "$UA" "https://crates.io/api/v1/crates/$1/$version"
}

# The crates.io logins that own <crate>, one per line; exit 1 when crates.io could not be asked.
owners_of() {
  local body
  body="$(curl -s -f -A "$UA" "https://crates.io/api/v1/crates/$1/owners")" || return 1
  printf '%s' "$body" | python3 -c 'import json,sys; print("\n".join(u["login"] for u in json.load(sys.stdin).get("users", []) if u.get("login")))'
}

# 0 when one of <expected logins> owns <crate>; 1 when only others own it; 2 when the owners are unknown.
owned_by_us() {
  local got want
  got="$(owners_of "$1")" || return 2
  [ -n "$got" ] || return 2
  for want in $2; do
    if printf '%s\n' "$got" | grep -qxF "$want"; then return 0; fi
  done
  return 1
}

# The login of the account CARGO_REGISTRY_TOKEN belongs to. The token goes to curl on stdin, never on the command line.
token_login() {
  printf 'header = "Authorization: %s"\n' "$CARGO_REGISTRY_TOKEN" \
    | curl -s -f -A "$UA" -K - "https://crates.io/api/v1/me" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["user"]["login"])'
}

bash tools/set-version.sh --check >/dev/null || { say "set-version.sh --check failed: fix the versions first"; exit 1; }
order="$(cargo metadata --no-deps --format-version 1 | python3 tools/publish-order.py)" || { say "could not work out the order"; exit 1; }
count="$(echo "$order" | wc -l)"

if [ "$mode" = list ]; then
  i=0
  for c in $order; do
    i=$((i+1))
    if [ "$(published "$c")" = 200 ]; then st="up at $version, owners: $(owners_of "$c" | tr '\n' ' ')"; else st="-"; fi
    printf '%3d %-24s %s\n' "$i" "$c" "$st"
  done
  exit 0
fi

if [ "$mode" = dry ]; then
  say "dry run: $count crates at $version"
  excl=""
  for n in $(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(" ".join(p["name"] for p in json.load(sys.stdin)["packages"] if p.get("publish")==[]))'); do excl="$excl --exclude $n"; done
  # shellcheck disable=SC2086
  cargo publish --dry-run --workspace $excl --locked >> "$log" 2>&1
  rc=$?
  say "dry run exit $rc; packaged $(grep -c '^ *Packaged ' "$log") and uploaded (dry) $(grep -c '^ *Uploading ' "$log") crates (counts include earlier runs in this log)"
  exit $rc
fi

[ -n "${CARGO_REGISTRY_TOKEN:-}" ] || { say "CARGO_REGISTRY_TOKEN is not set"; exit 1; }
ours="${BILINGUA_CRATES_OWNERS:-}"
if [ -z "$ours" ]; then
  ours="$(token_login)" || ours=""
  [ -n "$ours" ] || { say "could not ask crates.io whose token this is; set BILINGUA_CRATES_OWNERS to our crates.io login(s)"; exit 1; }
fi
say "publish: $count crates at $version, log $log, owners counted as ours: $ours"
started=0; [ -z "$from" ] && started=1
i=0
for c in $order; do
  i=$((i+1))
  [ "$started" = 0 ] && { [ "$c" = "$from" ] && started=1 || { say "[$i/$count] $c: before --from, skipped"; continue; }; }
  code="$(published "$c")"
  if [ "$code" = 200 ]; then
    owned_by_us "$c" "$ours"; own=$?
    if [ $own = 0 ]; then say "[$i/$count] $c $version: already on crates.io under our account, skipped"; continue; fi
    if [ $own = 1 ]; then say "[$i/$count] $c $version: on crates.io but NOT owned by $ours (owners: $(owners_of "$c" | tr '\n' ' ')); stopping"; exit 1; fi
    say "[$i/$count] $c $version: on crates.io but its owners could not be read; stopping (run again to retry)"; exit 1
  fi
  tries=0
  while :; do
    say "[$i/$count] $c $version: publishing"
    out="$(cargo publish -p "$c" --locked 2>&1)"; rc=$?
    printf '%s\n' "$out" >> "$log"
    if [ $rc = 0 ]; then say "[$i/$count] $c $version: published"; break; fi
    if echo "$out" | grep -q 'already exists\|already uploaded'; then
      # someone may have published this name between the check above and this publish: done only when it is ours
      owned_by_us "$c" "$ours"; own=$?
      if [ $own = 0 ]; then say "[$i/$count] $c $version: was already up under our account"; break; fi
      if [ $own = 1 ]; then say "[$i/$count] $c $version: already on crates.io but NOT owned by $ours (owners: $(owners_of "$c" | tr '\n' ' ')); stopping"; exit 1; fi
      say "[$i/$count] $c $version: already on crates.io but its owners could not be read; stopping (run again to retry)"; exit 1
    fi
    if echo "$out" | grep -qi '429\|too many'; then
      when="$(echo "$out" | grep -oE 'try again after [^.]+ GMT' | head -1 | sed 's/try again after //')"
      wait=600
      if [ -n "$when" ]; then t="$(date -d "$when" +%s 2>/dev/null || echo 0)"; [ "$t" -gt 0 ] && wait=$(( t - $(date +%s) + 15 )); fi
      [ "$wait" -lt 30 ] && wait=30
      say "[$i/$count] $c: rate limited${when:+ until $when}; sleeping ${wait}s"
      sleep "$wait"
      continue
    fi
    tries=$((tries+1))
    if [ $tries -gt 3 ]; then say "[$i/$count] $c: failed 4 times, stopping (see the log above); run again to resume"; exit 1; fi
    say "[$i/$count] $c: failed (exit $rc), retry $tries in $((60 * (1 << (tries-1))))s"
    sleep $((60 * (1 << (tries-1))))
  done
done
say "done: all $count crates are on crates.io at $version"
