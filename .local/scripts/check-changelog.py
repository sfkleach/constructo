#!/usr/bin/env python3
"""Check that the top section of CHANGELOG.md is a version release and matches Cargo.toml, and that the two crates
that are released together have the same version.

Usage: check-changelog.py

Exits with a non-zero status if:
- The first ## section heading contains the word 'unreleased', or
- The version in the first ## heading does not match the version in Cargo.toml, or
- The version in Cargo.lock does not match the version in Cargo.toml, or
- The version of constructo-runtime, in its Cargo.toml or in Cargo.lock, is not that of constructo, or
- constructo does not depend on exactly that version of constructo-runtime.

The two crates are published together, the runtime first, and the code that constructo codegen writes names the
version of the runtime that it needs from the version of constructo, so they cannot differ.
"""
import re
import sys


RUNTIME = "constructo-runtime"


def cargo_version(path: str = "Cargo.toml") -> str:
    """Return the first version in a manifest, which is that of its package."""
    with open(path) as f:
        for line in f:
            m = re.match(r'^version\s*=\s*"([^"]+)"', line)
            if m:
                return m.group(1)
    print(f"FAIL: Could not find version in {path}.")
    sys.exit(1)


def lock_version(package: str = "constructo") -> str:
    """Return the version of a package of the workspace recorded in Cargo.lock."""
    with open("Cargo.lock") as f:
        content = f.read()
    # Find the [[package]] block for the package and extract its version.
    m = re.search(r'\[\[package\]\]\s+name\s*=\s*"' + re.escape(package) + r'"\s+version\s*=\s*"([^"]+)"', content)
    if m:
        return m.group(1)
    print(f"FAIL: Could not find {package} package entry in Cargo.lock.")
    sys.exit(1)


def runtime_requirement() -> str:
    """Return the version requirement of the dependency of constructo on constructo-runtime."""
    with open("Cargo.toml") as f:
        for line in f:
            m = re.match(r'^' + re.escape(RUNTIME) + r'\s*=\s*\{[^}]*\bversion\s*=\s*"([^"]+)"', line)
            if m:
                return m.group(1)
    print(f"FAIL: Could not find the version of the dependency on {RUNTIME} in Cargo.toml.")
    sys.exit(1)


def check_runtime(version: str) -> None:
    """Fail unless constructo-runtime has the version of constructo, and constructo depends on exactly it."""
    runtime = cargo_version(f"{RUNTIME}/Cargo.toml")
    if runtime != version:
        print(
            f"FAIL: {RUNTIME}/Cargo.toml version '{runtime}' does not match "
            f"Cargo.toml version '{version}' — the two are released together."
        )
        sys.exit(1)
    locked = lock_version(RUNTIME)
    if locked != version:
        print(
            f"FAIL: Cargo.lock version '{locked}' of {RUNTIME} does not match "
            f"Cargo.toml version '{version}' — run 'cargo build' and commit Cargo.lock."
        )
        sys.exit(1)
    requirement = runtime_requirement()
    if requirement != f"={version}":
        print(
            f"FAIL: Cargo.toml depends on {RUNTIME} '{requirement}', "
            f"which should be '={version}'."
        )
        sys.exit(1)


def main() -> None:
    with open("CHANGELOG.md") as f:
        content = f.read()

    # Collect all level-2 headings.
    parts = re.split(r"\n(?=## )", content)
    headings = []
    for part in parts:
        if not part.startswith("## "):
            continue
        heading_end = part.index("\n") if "\n" in part else len(part)
        headings.append(part[:heading_end])

    if not headings:
        print("FAIL: No ## heading found in CHANGELOG.md.")
        sys.exit(1)

    # Extract version strings (vX.Y.Z) from all headings and check for duplicates.
    seen: dict[str, str] = {}
    for heading in headings:
        m = re.search(r"v\d+\.\d+\.\d+", heading)
        if not m:
            continue
        ver = m.group(0)
        if ver in seen:
            print(
                f"FAIL: Version '{ver}' appears more than once in CHANGELOG.md "
                f"('{seen[ver]}' and '{heading}')."
            )
            sys.exit(1)
        seen[ver] = heading

    heading = headings[0]
    if "unreleased" in heading.lower():
        print(
            f"FAIL: Top CHANGELOG section is '{heading}' "
            f"— release is not ready."
        )
        sys.exit(1)

    version = cargo_version()
    if version not in heading:
        print(
            f"FAIL: Cargo.toml version '{version}' not found in "
            f"CHANGELOG heading '{heading}'."
        )
        sys.exit(1)

    locked = lock_version()
    if locked != version:
        print(
            f"FAIL: Cargo.lock version '{locked}' does not match "
            f"Cargo.toml version '{version}' — run 'cargo build' and commit Cargo.lock."
        )
        sys.exit(1)

    check_runtime(version)

    print(
        f"OK: Top CHANGELOG section is '{heading}' (matches Cargo.toml v{version}, Cargo.lock v{locked}, "
        f"and {RUNTIME} v{version})."
    )
    sys.exit(0)


if __name__ == "__main__":
    main()
