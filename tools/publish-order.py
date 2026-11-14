"""Print the publishable workspace crates in an order crates.io accepts: every crate after the workspace crates it
depends on, dev-dependencies included (crates.io needs a dev-dependency with a version to exist already).

    cargo metadata --no-deps --format-version 1 | python3 tools/publish-order.py

Crates with `publish = false` (bilingua-py, the oracle) are left out. Ties go in name order, so the list is stable.
"""
import json
import sys

meta = json.load(sys.stdin)
pkgs = {p["name"]: p for p in meta["packages"]}
publishable = {n for n, p in pkgs.items() if p.get("publish") != []}
deps = {
    n: {d["name"] for d in pkgs[n]["dependencies"] if d["name"] in pkgs and d.get("path")} & publishable
    for n in publishable
}
for n, ds in deps.items():
    for d in ds:
        if d not in publishable:
            sys.exit(f"{n} depends on {d}, which has publish = false")
order, done = [], set()
while len(order) < len(publishable):
    ready = sorted(n for n in publishable - done if deps[n] <= done)
    if not ready:
        sys.exit("dependency cycle among: " + ", ".join(sorted(publishable - done)))
    order.extend(ready)
    done.update(ready)
print("\n".join(order))
