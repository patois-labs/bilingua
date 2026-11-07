#!/usr/bin/env bash
# Builds the bilingua Python wheel (manylinux_2_28, abi3, CPython 3.11 and up) for one branch on a Linux build host,
# tests it in a fresh venv there and copies it back.
#
#   M5=user@host build-wheel-m5.sh <branch> [out_dir]
#
# The branch goes over as a git bundle (the host needs no access to this repo). On the host, under ~/bili/:
#   wheelvenv/              maturin and ziglang (pinned below), made once with $PY -m venv; nothing system-wide
#   wheel-runs/<name>/      the checkout of the branch
#   wheels/<tip>/           the wheel and SHA256SUMS
#   wheeltest-<name>/       a fresh venv each run: the wheel, pytest, then bilingua-py/tests
# `maturin build` turns on the `extension-module` feature, so the module does not link libpython and needs no
# Python headers. `--zig` keeps the glibc floor at 2.28 whatever the host's own glibc is.
# Exit code: nonzero when the build or the tests failed. The wheel and SHA256SUMS land in out_dir (default
# ./wheels) only when both passed.
set -u
br="${1:?usage: M5=user@host build-wheel-m5.sh <branch> [out_dir]}"
out="${2:-./wheels}"
host="${M5:?set M5=user@host (the Linux build host)}"
name="wheel-$(echo "$br" | tr '/' '-')"
repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel) || exit 2
tmp=$(mktemp -d)
git -C "$repo" bundle create "$tmp/$name.bundle" "$br" >/dev/null 2>&1 || { echo "bundle failed for $br"; rm -rf "$tmp"; exit 2; }
ssh -o BatchMode=yes "$host" 'mkdir -p ~/bili/bundles ~/bili/wheel-runs ~/bili/wheels' || exit 2
scp -q -o BatchMode=yes "$tmp/$name.bundle" "$host:bili/bundles/$name.bundle" || { echo "scp failed"; rm -rf "$tmp"; exit 2; }
rm -rf "$tmp"
# ssh joins its arguments into one shell line: quote each one
tip=$(ssh -o BatchMode=yes "$host" bash -s -- "$(printf '%q' "$name")" "$(printf '%q' "$br")" "${PY:-python3.13}" <<'EOF'
set -u
name="$1"; br="$2"; py="$3"
MATURIN=1.16.0
ZIGLANG=0.15.2
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
run=~/bili/wheel-runs/$name; log=~/bili/wheel-runs/$name.log
exec 9>~/bili/wheel-runs/$name.lock; flock 9
: > "$log"
(
  if [ ! -d "$run/.git" ]; then git clone -q --no-checkout ~/bili/bundles/$name.bundle "$run"; fi
  cd "$run" || exit 1
  git fetch -q ~/bili/bundles/$name.bundle "$br:refs/remotes/b/tip" --force
  git checkout -q --force refs/remotes/b/tip && git clean -qfdx
  # cat-os keeps Bilingua under bilingua/; the bilingua repo is the workspace itself
  ws=.; [ -f bilingua/Cargo.toml ] && ws=bilingua
  tip=$(git rev-parse --short HEAD)
  echo "tip $tip"
  venv=~/bili/wheelvenv
  if [ ! -x "$venv/bin/maturin" ] || ! "$venv/bin/maturin" --version | grep -q " $MATURIN"; then
    "$py" -m venv "$venv" && "$venv/bin/pip" install -q "maturin==$MATURIN" "ziglang==$ZIGLANG" || exit 1
  fi
  dest=~/bili/wheels/$tip; rm -rf "$dest"; mkdir -p "$dest"
  CARGO_TARGET_DIR=~/bili/targets/$name "$venv/bin/maturin" build --release --locked --compatibility manylinux_2_28 --zig \
    -i "$venv/bin/python" -m "$ws/bilingua-py/Cargo.toml" -o "$dest" || { echo "build=1"; exit 1; }
  echo "build=0"
  whl=$(ls "$dest"/bilingua-*.whl) || exit 1
  test=~/bili/wheeltest-$name; rm -rf "$test"
  "$py" -m venv "$test" && "$test/bin/pip" install -q "$whl" pytest || { echo "install=1"; exit 1; }
  "$test/bin/python" -I -c "import bilingua; print('module', bilingua.version(), bilingua.profiles())"
  (cd "$ws/bilingua-py" && "$test/bin/python" -m pytest -q -p no:cacheprovider tests); t=$?
  echo "pytest=$t"
  (cd "$dest" && sha256sum ./*.whl > SHA256SUMS)
  [ $t -eq 0 ] || exit 1
) >> "$log" 2>&1
rc=$?
tail -n 15 "$log" >&2
echo "log: $log" >&2
[ $rc -eq 0 ] && grep -m1 '^tip ' "$log" | cut -d' ' -f2
exit $rc
EOF
) || { echo "wheel build or tests failed"; exit 1; }
mkdir -p "$out"
scp -q -o BatchMode=yes "$host:bili/wheels/$tip/*" "$out/" || { echo "copy back failed"; exit 2; }
echo "wheel for $br ($tip) in $out:"
ls -l "$out"
