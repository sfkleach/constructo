#!/usr/bin/env bash
# Functional tests for constructo.
#
# Usage: functest.sh <binary> [mode]
#   binary  path to the constructo binary to test
#   mode    accepted for consistency with the Justfile; currently unused
#
# Exit code: 0 if all tests passed, 1 if any failed.

set -euo pipefail

CONSTRUCTO="${1:?usage: functest.sh <binary> [mode]}"
EXAMPLES="$(dirname "$0")/../syntax/examples/types"
VALUES="$(dirname "$0")/../syntax/examples/values"

PASS=0
FAIL=0

pass() { echo "  PASS: $1"; (( PASS++ )) || true; }
fail() { echo "  FAIL: $1"; (( FAIL++ )) || true; }

# Runs constructo on a file and compares its exit code with the expected one.
expect_exit() {
    local name="$1" expected="$2"; shift 2
    local actual=0
    "$CONSTRUCTO" "$@" >/dev/null 2>&1 || actual=$?
    if [[ "$actual" -eq "$expected" ]]; then
        pass "$name"
    else
        fail "$name (expected exit $expected, got $actual)"
    fi
}

echo "=== constructo functional tests ($CONSTRUCTO) ==="
echo ""

echo "--- valid types ---"
for f in "$EXAMPLES"/valid/*.types; do
    expect_exit "valid/$(basename "$f")" 0 check "$f"
done
echo ""

echo "--- invalid types ---"
for f in "$EXAMPLES"/invalid/*.types; do
    expect_exit "invalid/$(basename "$f")" 1 check "$f"
done
echo ""

echo "--- parses but invalid ---"
for f in "$EXAMPLES"/parses-but-invalid/*.types; do
    expect_exit "parses-but-invalid/$(basename "$f")" 1 check "$f"
done
echo ""

echo "--- valid values ---"
for f in "$VALUES"/valid/*.values; do
    expect_exit "values/valid/$(basename "$f")" 0 check "$f"
done
echo ""

echo "--- invalid values ---"
for f in "$VALUES"/invalid/*.values; do
    expect_exit "values/invalid/$(basename "$f")" 1 check "$f"
done
echo ""

echo "--- mixed types and values ---"
expect_exit "types and values together" 0 check "$EXAMPLES/valid/shapes.types" "$VALUES/valid/tagged.values"
expect_exit "values are not yet checked against types" 0 \
    check "$EXAMPLES/valid/message.types" "$VALUES/valid/untagged.values"
expect_exit "a values syntax error after types exits 1" 1 \
    check "$EXAMPLES/valid/shapes.types" "$VALUES/invalid/missing-separator.values"
expect_exit "a values file needs no types file" 0 check "$VALUES/valid/single-value.values"
expect_exit "stdin may hold values" 0 check --stdin < "$VALUES/valid/single-value.values"
echo ""

echo "--- prelude ---"
expect_exit "prelude supplies Option" 0 check "$EXAMPLES/valid/mutual.types"
expect_exit "--no-prelude hides Option" 1 check --no-prelude "$EXAMPLES/valid/mutual.types"
expect_exit "prelude supplies Bool" 0 check "$EXAMPLES/valid/uses-bool.types"
expect_exit "prelude names cannot be redeclared" 1 check "$EXAMPLES/parses-but-invalid/redeclares-bool.types"
expect_exit "--no-prelude allows redeclaring Bool" 0 \
    check --no-prelude "$EXAMPLES/parses-but-invalid/redeclares-bool.types"
expect_exit "--no-prelude with a self-contained file" 0 check --no-prelude "$EXAMPLES/valid/shapes.types"
echo ""

echo "--- invocation ---"
expect_exit "missing file is a usage error" 2 check "$EXAMPLES/valid/no-such-file.types"
expect_exit "no arguments is a usage error" 2 check
expect_exit "multiple valid files share a namespace" 0 check "$EXAMPLES/valid/shapes.types" "$EXAMPLES/valid/list.types"

# An earlier syntax error wins over a later unreadable input, but an earlier unreadable input is still a usage error.
expect_exit "syntax error before a missing file exits 1" 1 \
    check "$EXAMPLES/invalid/missing-semicolon.types" "$EXAMPLES/valid/no-such-file.types"
expect_exit "missing file before a syntax error exits 2" 2 \
    check "$EXAMPLES/valid/no-such-file.types" "$EXAMPLES/invalid/missing-semicolon.types"
expect_exit "syntax error stops before stdin is read" 1 \
    check --stdin "$EXAMPLES/invalid/missing-semicolon.types" < /dev/null

stdin_exit() {
    local name="$1" expected="$2" input="$3"; shift 3
    local actual=0
    printf '%s\n' "$input" | "$CONSTRUCTO" check --stdin "$@" >/dev/null 2>&1 || actual=$?
    if [[ "$actual" -eq "$expected" ]]; then
        pass "$name"
    else
        fail "$name (expected exit $expected, got $actual)"
    fi
}
stdin_exit "valid stdin" 0 "enum A = X;"
stdin_exit "invalid stdin" 1 "enum A = X"
stdin_exit "stdin shares a namespace with files" 0 "enum A = X(Shape);" "$EXAMPLES/valid/shapes.types"

out="$("$CONSTRUCTO" check "$EXAMPLES/invalid/missing-semicolon.types" 2>&1 >/dev/null || true)"
if [[ "$out" == ERROR:*"missing-semicolon.types, line "*", col "* ]]; then
    pass "diagnostic format"
else
    fail "diagnostic format (got '$out')"
fi

quiet="$("$CONSTRUCTO" check "$EXAMPLES/valid/shapes.types" 2>&1)"
if [[ -z "$quiet" ]]; then
    pass "no output on success"
else
    fail "no output on success (got '$quiet')"
fi
loud="$("$CONSTRUCTO" check -v "$EXAMPLES/valid/shapes.types" 2>&1)"
if [[ "$loud" == *"OK: $EXAMPLES/valid/shapes.types"* && "$loud" == *"OK: checked 1 file(s)"* ]]; then
    pass "-v reports each file and a final count"
else
    fail "-v reports each file and a final count (got '$loud')"
fi
if [[ "$loud" != *"<prelude>"* ]]; then
    pass "-v does not report the prelude"
else
    fail "-v does not report the prelude (got '$loud')"
fi
loud="$("$CONSTRUCTO" check -v "$EXAMPLES/valid/shapes.types" "$VALUES/valid/tagged.values" 2>&1)"
if [[ "$loud" == *"OK: $EXAMPLES/valid/shapes.types (data-types)"* \
    && "$loud" == *"OK: $VALUES/valid/tagged.values (data-values)"* ]]; then
    pass "-v shows the kind of each file"
else
    fail "-v shows the kind of each file (got '$loud')"
fi
loud="$("$CONSTRUCTO" check -v "$VALUES/invalid/missing-separator.values" 2>&1 || true)"
if [[ "$loud" == *"missing-separator.values, line 2, col 3)"* && "$loud" == *"FAILED: "*"(data-values)"* ]]; then
    pass "-v names the file, location and kind of a values error"
else
    fail "-v names the file, location and kind of a values error (got '$loud')"
fi
loud="$("$CONSTRUCTO" check -v "$EXAMPLES/valid/shapes.types" "$EXAMPLES/valid/list.types" 2>&1)"
if [[ "$loud" == *"OK: checked 2 file(s)"* ]]; then
    pass "-v counts several files"
else
    fail "-v counts several files (got '$loud')"
fi
loud="$(printf 'enum A = X;\n' | "$CONSTRUCTO" check -v --stdin 2>&1)"
if [[ "$loud" == *"OK: <stdin>"* ]]; then
    pass "-v names stdin as <stdin>"
else
    fail "-v names stdin as <stdin> (got '$loud')"
fi
loud="$("$CONSTRUCTO" check -v "$EXAMPLES/parses-but-invalid/duplicate-variant.types" \
    "$EXAMPLES/valid/shapes.types" 2>/dev/null || true)"
if [[ "$loud" == *"FAILED: "*duplicate-variant.types* && "$loud" == *"OK: "*shapes.types* \
    && "$loud" != *"checked"* ]]; then
    pass "-v marks failed files and gives no final OK"
else
    fail "-v marks failed files and gives no final OK (got '$loud')"
fi
err="$("$CONSTRUCTO" check -v "$EXAMPLES/valid/shapes.types" "$EXAMPLES/invalid/missing-semicolon.types" \
    "$EXAMPLES/invalid/no-variants.types" 2>&1 || true)"
if [[ "$err" == *"in enum Shape"* && "$err" != *"no-variants"* ]]; then
    pass "stops at the first syntax error and names the enum"
else
    fail "stops at the first syntax error and names the enum (got '$err')"
fi
echo ""

TOTAL=$(( PASS + FAIL ))
echo "Results: $PASS/$TOTAL passed"
if [[ $FAIL -gt 0 ]]; then
    echo "FAILED: $FAIL test(s) failed"
    exit 1
fi
