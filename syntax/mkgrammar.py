#!/usr/bin/env python3
"""Generate grammar.md from a template and the grammar sources.

The template's {{TYPES_RULES}} and {{VALUES_RULES}} placeholders are replaced with the names of the rules defined in
the data-types and data-values grammar sources, so the text follows the grammars when they change.
"""

import argparse
import re
import sys
import textwrap


def rule_names(path):
    """Return the rule names, which are given by the '// Name' lines that precede each diagram."""
    with open(path, encoding="utf-8") as f:
        text = f.read()
    # The first diagram, for the start rule, follows a title line that contains spaces. It is named by its label.
    start = re.search(r"Comment\('(\w+)'\)", text)
    return ([start.group(1)] if start else []) + re.findall(r"^// (\S+)\s*$", text, re.M)


def format_rules(names):
    return textwrap.fill(", ".join(f"`{name}`" for name in names) + ".", width=118)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--template", required=True)
    parser.add_argument("--types", required=True, help="The data-types grammar source.")
    parser.add_argument("--values", required=True, help="The data-values grammar source.")
    args = parser.parse_args()

    with open(args.template, encoding="utf-8") as f:
        text = f.read()
    replacements = {
        "{{TYPES_RULES}}": format_rules(rule_names(args.types)),
        "{{VALUES_RULES}}": format_rules(rule_names(args.values)),
    }
    for placeholder, value in replacements.items():
        if placeholder not in text:
            sys.exit(f"{args.template}: missing placeholder {placeholder}")
        text = text.replace(placeholder, value)
    sys.stdout.write(text)


if __name__ == "__main__":
    main()
