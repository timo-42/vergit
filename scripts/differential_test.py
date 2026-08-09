#!/usr/bin/env python3
"""Differentially test the `pep440` crate against Python's `packaging`.

Generates a large corpus of version strings — valid, weirdly spelled, and
outright junk — then checks that the crate agrees with the reference
implementation on two things:

  1. validity and normalization (`Display` output), and
  2. ordering (`Ord`).

Requires `packaging` and a Rust toolchain. Run from the repository root:

    pip install packaging && python3 scripts/differential_test.py
"""

from __future__ import annotations

import itertools
import random
import subprocess
import sys
from pathlib import Path

try:
    from packaging.version import InvalidVersion, Version
except ImportError:
    sys.exit("this script needs `packaging`: pip install packaging")

ROOT = Path(__file__).resolve().parent.parent
SEED = 7

# `packaging` uses Python's arbitrary-precision ints; the crate uses u64 so the
# binary stays small. Numeric segments above u64::MAX are rejected outright
# rather than silently truncated, which is the safe failure. These inputs are
# checked to still be *rejected* rather than compared for equality.
KNOWN_DIVERGENCES = {
    "99999999999999999999999.0",
}


def corpus() -> list[str]:
    """Every spelling the PEP allows, plus junk that must be rejected."""
    random.seed(SEED)

    epochs = ["", "0!", "1!", "23!"]
    releases = ["1", "1.0", "0.1", "1.2.3", "01.2", "1.2.3.4", "10.0.0", "1.0.0.0"]
    labels = ["a", "b", "c", "rc", "alpha", "beta", "pre", "preview", "A", "RC", "Alpha"]
    seps = ["", ".", "-", "_"]
    pres = [""] + [
        f"{lead}{label}{trail}{n}"
        for lead in seps
        for label in labels
        for trail in seps
        for n in ["", "0", "1", "07", "12"]
    ]
    posts = ["", "-1", "-09", ".post1", "-post2", "_post_3", ".post", "post",
             ".rev4", "-r5", "_R_6", ".REV"]
    devs = ["", ".dev", ".dev0", "-dev1", "_dev_09", "dev", ".DEV2"]
    locals_ = ["", "+abc", "+1", "+abc.5", "+ABC-5_6", "+0001", "+a.b.c",
               "+ubuntu-1", "+g1a2b3c4"]

    out = [f"v{r}" for r in releases] + ["V1.0", " 1.0 "]
    for epoch, release in itertools.product(epochs, releases):
        for pre in random.sample(pres, 40):
            for post in random.sample(posts, 4):
                for dev in random.sample(devs, 3):
                    for local in random.sample(locals_, 2):
                        out.append(f"{epoch}{release}{pre}{post}{dev}{local}")

    out += [
        "", " ", "abc", "1.0+", "1.0.", "1..0", "-1.0", "1.0++a", "1.0+_",
        "1.2.3four", "1.0 dev1", "v", "!1.0", "1!", "1.0-", "+1", "1.0a-",
        "..", "1.0#", "1.0+a b", "1.0+é", "99999999999999999999999.0",
    ]
    # Deduplicate, preserving order. Tabs and newlines would break the
    # line-oriented protocol used to talk to the Rust helpers.
    return [v for v in dict.fromkeys(out) if not (set(v) & set("\t\r\n"))]


def reference_normalize(versions: list[str]) -> list[str]:
    out = []
    for v in versions:
        try:
            out.append(str(Version(v)))
        except InvalidVersion:
            out.append("<INVALID>")
    return out


def run_example(name: str, stdin: str) -> list[str]:
    result = subprocess.run(
        ["cargo", "run", "--quiet", "--release", "--example", name, "-p", "pep440"],
        cwd=ROOT,
        input=stdin,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        sys.exit(f"`{name}` failed:\n{result.stderr}")
    return result.stdout.splitlines()


def report(kind: str, inputs: list[str], want: list[str], got: list[str]) -> int:
    if len(want) != len(got):
        sys.exit(f"{kind}: got {len(got)} lines, expected {len(want)}")
    mismatches = [
        (i, w, g) for i, (w, g) in enumerate(zip(want, got)) if w != g
    ]
    for i, w, g in mismatches[:20]:
        print(f"  {kind} {inputs[i]!r}: packaging={w!r} pep440={g!r}")
    if len(mismatches) > 20:
        print(f"  ... and {len(mismatches) - 20} more")
    print(f"{kind}: {len(want) - len(mismatches)}/{len(want)} agree")
    return len(mismatches)


def check_divergences(versions: list[str]) -> int:
    """The documented divergences must still behave exactly as documented."""
    cases = sorted(KNOWN_DIVERGENCES & set(versions))
    if not cases:
        return 0
    got = run_example("normalize", "\n".join(cases) + "\n")
    bad = [(v, g) for v, g in zip(cases, got) if g != "<INVALID>"]
    for v, g in bad:
        print(f"  divergence {v!r}: expected rejection, got {g!r}")
    print(f"documented divergences: {len(cases) - len(bad)}/{len(cases)} still rejected")
    return len(bad)


def main() -> int:
    versions = corpus()
    print(f"corpus: {len(versions)} version strings")

    failures = check_divergences(versions)
    versions = [v for v in versions if v not in KNOWN_DIVERGENCES]

    want = reference_normalize(versions)
    got = run_example("normalize", "\n".join(versions) + "\n")
    failures += report("normalize", versions, want, got)

    # Ordering, over the inputs both agree are valid.
    valid = [v for v, n in zip(versions, want) if n != "<INVALID>"]
    print(f"ordering: {len(valid)} valid versions")
    # Both sorts are stable and start from the same order, so equal versions
    # stay in the same relative position on both sides.
    want_sorted = [str(Version(v)) for v in sorted(valid, key=Version)]
    got_sorted = run_example("sortlines", "\n".join(valid) + "\n")
    failures += report("ordering", valid, want_sorted, got_sorted)

    if failures:
        print(f"\nFAILED: {failures} disagreements with packaging")
        return 1
    print("\nOK: pep440 matches packaging on normalization and ordering")
    return 0


if __name__ == "__main__":
    sys.exit(main())
