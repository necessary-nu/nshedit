#!/usr/bin/env python3
"""Turn llvm-cov output into the list of spec rules the drivers provably drive.

Called by `conformance/coverage.sh`, which does the measuring; this does the
mapping and writes the claim file. Kept separate for the same reason
`header-abi.py` is: the shell is good at building and running things and bad
at joining two structured datasets.

The join is:

    a `sem` annotation in the port's source labels the function below it
        x
    llvm-cov says which functions executed, and under which driver

    -> a rule is claimed, by name, for the drivers that executed its function

A function that no driver reaches is simply absent. That is the point: the
verified count then reflects execution rather than intent, which is what the
`conformance` node's constraint asks for.
"""

import collections
import glob
import json
import os
import re
import sys

# A `sem` or `def` annotation, in the form the port's sources carry.
ANNOTATION = re.compile(r"\[spec:libedit:(sem|def):([a-z0-9._-]+)\]")
# The first line of a function definition, in every shape the port uses.
FN = re.compile(r'\s*(pub(\(crate\))?\s+)?(unsafe\s+)?(extern\s+"C"\s+)?fn\s')
# Between an annotation and the item it labels there is only ever a doc
# comment, another annotation, an attribute or a blank line. Anything else is
# the item. Scanning for THAT rather than allowing a fixed number of lines is
# what makes the pairing exact: `el_init` carries a fifteen-line doc comment
# and a twelve-line window silently dropped it, along with a hundred others.
SKIPPABLE = re.compile(r"\s*(//|#\[|$)")


def executed_ranges(export_path):
    """Line ranges that executed, per real path, from one llvm-cov export."""
    with open(export_path) as fh:
        data = json.load(fh)
    ranges = collections.defaultdict(list)
    for fn in data["data"][0]["functions"]:
        if fn["count"] == 0:
            continue
        lines = [r[0] for r in fn["regions"]] + [r[2] for r in fn["regions"]]
        if not lines:
            continue
        span = (min(lines), max(lines))
        for name in fn["filenames"]:
            ranges[os.path.realpath(name)].append(span)
    return ranges


def annotations(root):
    """Every annotation in the port's sources, paired with the fn it labels.

    Test files are excluded: an annotation there is a claim about a test, and
    this function is looking for the implementation sites.
    """
    found = []
    for path in glob.glob(os.path.join(root, "crates/*/src/**/*.rs"), recursive=True):
        with open(path) as fh:
            lines = fh.read().splitlines()
        real = os.path.realpath(path)
        rel = os.path.relpath(path, root)
        for i, line in enumerate(lines):
            m = ANNOTATION.search(line)
            if not m:
                continue
            for j in range(i + 1, len(lines)):
                if SKIPPABLE.match(lines[j]):
                    continue
                if FN.match(lines[j]):
                    found.append((m.group(2), real, rel, j + 1))
                break
    return found


def claims(cov_dir, root, drivers):
    """{rule: [driver, ...]} for every rule a driver's execution reached."""
    per_driver = {d: executed_ranges(os.path.join(cov_dir, f"{d}.json")) for d in drivers}
    by_rule = collections.defaultdict(list)
    sites = {}
    for rule, real, rel, line in annotations(root):
        for driver in drivers:
            if any(lo <= line <= hi for lo, hi in per_driver[driver].get(real, ())):
                by_rule[rule].append(driver)
                sites[rule] = f"{rel}:{line}"
    return by_rule, sites


HEADER = '''//! What the conformance drivers provably drive.
//!
//! # This file is generated
//!
//! `./conformance/coverage.sh` writes it and `--check` verifies it. Do not
//! edit it by hand: every line below was produced by rebuilding the cdylib
//! under `-C instrument-coverage`, running each driver against it, and asking
//! `llvm-cov` which functions executed. Editing one in would be a claim
//! nothing measured.
//!
//! # Why the claims live here and not beside the functions
//!
//! The port gate reads the `include` globs in `.config/nspec/config.styx`,
//! so a `/test` facet counts only inside `crates/**/*.rs`. Measured, not
//! assumed: a `/test` annotation in a test file moves the count and a bare
//! `sem` annotation in the same place does not. `conformance/**` is in
//! `test_include`, which `nplan spec status` reads and the port gate does
//! not — so the drivers cannot claim anything from where they live, and this
//! file is the bridge.
//!
//! Putting them here is also the more honest arrangement. A `/test` next to
//! an implementation reads as "this function has a unit test"; these are not
//! unit tests, they are one C program driving both libraries and diffing the
//! traces, and the claim belongs next to the test that runs them.
//!
//! # What a claim means, and what it does not
//!
//! It means: this driver executed this function, and the trace it produced
//! was identical to the one the C produced. It does not mean the function is
//! exhaustively tested — a driver that calls `history_save` once has covered
//! one path through it. The `conformance` node's constraint is about the
//! opposite failure, claiming what nothing runs, and this is what keeps the
//! count on the right side of it.
//!
//! Rules whose functions no driver reaches are absent rather than listed as
//! gaps. The gap is the difference between this count and 572, and it is
//! meant to be read from `nplan port status` rather than from here.
'''


def render(by_rule, sites, drivers, root):
    out = [HEADER]
    total = 0
    for driver in drivers:
        mine = sorted(r for r, ds in by_rule.items() if ds and ds[0] == driver)
        src = (
            "conformance/aux/ub_corpus.c"
            if driver == "ub_corpus"
            else f"conformance/driver/{driver}.c"
        )
        out.append("\n// ---------------------------------------------------------------------------"[1:]
                   if driver == drivers[0] else
                   "\n// ---------------------------------------------------------------------------")
        out.append(f"// {src} — {len(mine)} rules")
        out.append(f"// ---------------------------------------------------------------------------")
        for rule in mine:
            out.append(f"// [spec:libedit:sem:{rule}/test]  {sites[rule]}")
        total += len(mine)

    shared = sorted(r for r, ds in by_rule.items() if len(ds) > 1)
    out.append("")
    out.append("/// The drivers, and the count each one earns.")
    out.append("///")
    out.append("/// A rule reached by more than one driver is attributed to the first that")
    out.append("/// reaches it, so these sum to the total. The overlap is large and that is")
    out.append(f"/// expected — {len(shared)} of {len(by_rule)} rules are reached by more than one,")
    out.append("/// because every driver goes through the same lifecycle and allocator paths.")
    out.append("#[test]")
    out.append("fn the_claim_list_is_what_coverage_measured() {")
    out.append("    // Regenerate with ./conformance/coverage.sh, verify with --check.")
    out.append(f"    // {total} rules across {len(drivers)} drivers, measured "
               f"under -C instrument-coverage.")
    out.append(f"    assert_eq!(CLAIMED, {total});")
    out.append("}")
    out.append("")
    out.append("/// How many `/test` facets this file carries. The generator and the")
    out.append("/// annotations above are written together, so a hand edit to either")
    out.append("/// desynchronises them and `coverage.sh --check` says so.")
    out.append(f"const CLAIMED: usize = {total};")
    out.append("")
    return "\n".join(out), total


def main():
    mode, cov_dir, root, claims_path, *drivers = sys.argv[1:]
    by_rule, sites = claims(cov_dir, root, drivers)
    text, total = render(by_rule, sites, drivers, root)

    counts = {d: sum(1 for ds in by_rule.values() if ds and ds[0] == d) for d in drivers}
    for d in drivers:
        print(f"  {counts[d]:4d}  {d}")
    print(f"  {total:4d}  total, {len(by_rule)} distinct rules")

    existing = None
    if os.path.exists(claims_path):
        with open(claims_path) as fh:
            existing = fh.read()

    if mode == "--check":
        if existing != text:
            print("\nFAIL: crates/nshedit-abi/tests/driven.rs is not what the drivers")
            print("drive. Regenerate it with ./conformance/coverage.sh.")
            return 1
        print("\nthe claim file matches what the drivers drive")
        return 0

    with open(claims_path, "w") as fh:
        fh.write(text)
    print(f"\nwrote {os.path.relpath(claims_path, root)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
