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
PERMISSIVE_CHOICE = {"MIT", "Apache-2.0"}
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
    # Top-level OR alternatives: one permissive choice settles the crate.
    alternatives = [a.strip(" ()") for a in re.split(r"\s+OR\s+", lic)] if " AND " not in lic else [lic]
    if any(a in PERMISSIVE_CHOICE for a in alternatives):
        continue
    if COPYLEFT.search(lic) or lic == "UNKNOWN":
        flagged.append((crate, lic))
    for term in re.split(r"\s+(?:AND|OR|WITH)\s+", lic.replace("(", "").replace(")", "")):
        if term not in PERMISSIVE_CHOICE and term != "LLVM-exception":
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
tag = " AND ".join(["(MIT OR Apache-2.0)"] + sorted(needed))
print(f"  suggested License: {tag}")
print()
' "$pkg"
done
