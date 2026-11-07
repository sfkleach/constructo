#!/usr/bin/env python3
"""Mechanical checks of the Markdown files in docs/specs/, for the review-specs skill.

It reports, one problem per line as FILE:LINE: MESSAGE:

- a relative link whose file does not exist, or whose #anchor is not a heading of that file;
- a path in backticks that starts with docs/, src/, tests/ or syntax/ and does not exist;
- a decision named by number ("decision 30", "Decisions 24 to 28") that has no file in docs/decisions/;
- a line longer than 120 characters outside a code block, a tab, trailing whitespace other than the single space of
  a Markdown line break, CRLF line endings, and a file that does not end with a newline.

It changes nothing. Run it from the root of the repository:

    python3 .claude/skills/review-specs/check_specs.py [FILE ...]

With no files it checks every docs/specs/*.md. The exit code is 1 if anything was reported, and 0 otherwise.
"""

import re
import sys
from pathlib import Path

MAX_LINE = 120
LINK = re.compile(r"(?<!!)\[[^\]]*\]\(([^)\s]+)\)")
CODE_PATH = re.compile(r"`((?:docs|src|tests|syntax)/[^`\s]*)`")
# "decisions 24 to 28" is a range, and "decisions 4, 25 and 26" a list.
DECISIONS = re.compile(r"[Dd]ecisions? (\d+)((?:(?:,| to| and) \d+)*)")


def slug(heading: str) -> str:
    """The anchor that GitHub gives a heading: lowercase, punctuation removed, spaces made hyphens."""
    text = heading.strip().lower()
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def anchors(path: Path) -> set:
    found, seen, in_code = set(), {}, False
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("```"):
            in_code = not in_code
            continue
        match = re.match(r"#{1,6} (.*)", line)
        if match and not in_code:
            base = slug(match.group(1))
            count = seen.get(base, 0)
            seen[base] = count + 1
            found.add(base if count == 0 else f"{base}-{count}")
    return found


def decision_exists(number: int) -> bool:
    return any(Path("docs/decisions").glob(f"{number:04d}-*"))


def check(path: Path) -> list:
    problems = []
    raw = path.read_bytes()
    if b"\r\n" in raw:
        problems.append(f"{path}:1: has CRLF line endings")
    if raw and not raw.endswith(b"\n"):
        problems.append(f"{path}:1: does not end with a newline")
    in_code = False
    for number, line in enumerate(raw.decode("utf-8").splitlines(), start=1):
        where = f"{path}:{number}"
        if line.startswith("```"):
            in_code = not in_code
        if "\t" in line:
            problems.append(f"{where}: has a tab")
        stripped = line.rstrip(" ")
        if line != stripped and not (len(line) - len(stripped) == 1 and stripped):
            problems.append(f"{where}: has trailing whitespace that is not a single-space line break")
        if len(line) > MAX_LINE and not in_code:
            problems.append(f"{where}: is {len(line)} characters long, more than {MAX_LINE}")
        if in_code:
            continue
        for target in LINK.findall(line):
            if re.match(r"[a-z]+:", target):
                continue
            file_part, _, anchor = target.partition("#")
            linked = (path.parent / file_part).resolve() if file_part else path.resolve()
            if not linked.exists():
                problems.append(f"{where}: links to {target}, which does not exist")
            elif anchor and linked.suffix == ".md" and anchor not in anchors(linked):
                problems.append(f"{where}: links to #{anchor}, which is not a heading of {linked.name}")
        for mentioned in CODE_PATH.findall(line):
            if not any(ch in mentioned for ch in "*?<>") and not Path(mentioned.rstrip("/.,")).exists():
                problems.append(f"{where}: names {mentioned}, which does not exist")
        for first, more in DECISIONS.findall(line):
            numbers, previous = {int(first)}, int(first)
            for joint, n in re.findall(r"(,| to| and) (\d+)", more):
                n = int(n)
                numbers.update(range(previous, n + 1) if joint == " to" else [n])
                previous = n
            for n in sorted(numbers):
                if not decision_exists(n):
                    problems.append(f"{where}: names decision {n}, which has no file in docs/decisions/")
    return problems


def main() -> int:
    files = [Path(name) for name in sys.argv[1:]] or sorted(Path("docs/specs").glob("*.md"))
    problems = [problem for path in files for problem in check(path)]
    for problem in problems:
        print(problem)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
