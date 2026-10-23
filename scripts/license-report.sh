#!/bin/bash
# Licence report for what each yAIst binary actually links, following the
# openSUSE packaging skill's Rust procedure: `cargo tree -e normal` scoped to
# the built package and a Linux target, then every crate's `license` field.
#
#   scripts/license-report.sh            report for yaist and yaist-gui
#   scripts/license-report.sh --offline  inside an rpmbuild with vendor/
#
# Prints the distinct licence expressions, flags copyleft that is not merely
# one OR choice, and suggests the spec's License: tag. Re-run on every
# dependency update: a re-vendor can bring in new licences.
set -euo pipefail
cd "$(dirname "$0")/.."

CARGO=(cargo)
if ! command -v cargo >/dev/null; then
    CARGO=(scripts/build-in-container.sh)
fi
TARGET="${YAIST_LICENSE_TARGET:-x86_64-unknown-linux-gnu}"

for pkg in yaist yaist-gui; do
    "${CARGO[@]}" tree "$@" -p "$pkg" -e normal --prefix none --no-dedupe --target "$TARGET" --format '{p}|{l}' 2>/dev/null |
        sort -u | python3 -c '
import re, sys
pkg = sys.argv[1]
own = {"yaist", "yaist-core", "yaist-modules", "yaist-agent", "yaist-ui", "yaist-gui"}
COPYLEFT = re.compile(r"\b(A?GPL|LGPL|MPL|EPL|CDDL|CC-BY-SA|SSPL|OSL)")
OURS = "Apache-2.0"
exprs, flagged, needed = {}, [], set()
for line in sys.stdin:
    line = line.strip()
    if "|" not in line:
        continue
    crate, lic = line.split("|", 1)
    name = crate.split()[0]
    if name in own:
        continue
    lic = (lic or "UNKNOWN").replace("/", " OR ").strip()
    lic = re.sub(r"\s+", " ", lic)
    exprs.setdefault(lic, []).append(crate.replace(" (proc-macro)", ""))
    # A top-level OR choice: take Apache-2.0 when offered, else MIT, else the
    # first permissive alternative (copyleft only when nothing else is offered).
    if " AND " not in lic:
        alternatives = [a.strip(" ()") for a in re.split(r"\s+OR\s+", lic)]
        alternatives = [a.split(" WITH ")[0] for a in alternatives]
        if OURS in alternatives:
            continue
        free = sorted((a for a in alternatives if not COPYLEFT.search(a)), key=lambda a: a != "MIT")
        if free:
            needed.add(free[0])
            continue
        flagged.append((crate, lic))
        needed.update(alternatives)
        continue
    if COPYLEFT.search(lic) or lic == "UNKNOWN":
        flagged.append((crate, lic))
    for term in re.split(r"\s+(?:AND|WITH)\s+", lic.replace("(", "").replace(")", "")):
        term = term.split(" OR ")[0] if " OR " in term else term
        if term != OURS and term != "LLVM-exception":
            needed.add(term)
print(f"== {pkg}: {sum(len(v) for v in exprs.values())} third-party crates linked")
for lic, crates in sorted(exprs.items(), key=lambda kv: -len(kv[1])):
    print(f"  {len(crates):4}  {lic}")
if pkg == "yaist-gui":
    needed.add("OFL-1.1")  # bundled fonts
if flagged:
    print("  COPYLEFT / UNKNOWN without a permissive choice:")
    for crate, lic in flagged:
        print(f"    {crate}: {lic}")
else:
    print("  no copyleft without a permissive choice")
tag = " AND ".join([OURS] + sorted(needed))
print(f"  suggested License: {tag}")
print()
' "$pkg"
done
