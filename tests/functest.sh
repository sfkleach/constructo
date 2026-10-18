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

# Examples that cannot pass until the standard prelude exists (Step 3 of the task).
NEEDS_PRELUDE=(mutual.types)

needs_prelude() {
    local file
    for file in "${NEEDS_PRELUDE[@]}"; do
        [[ "$file" == "$1" ]] && return 0
    done
    return 1
}

echo "=== constructo functional tests ($CONSTRUCTO) ==="
echo ""

echo "--- valid types ---"
for f in "$EXAMPLES"/valid/*.types; do
    base="$(basename "$f")"
    if needs_prelude "$base"; then
        echo "  SKIP: valid/$base (needs the prelude)"
    else
        expect_exit "valid/$base" 0 check "$f"
    fi
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

echo "--- invocation ---"
expect_exit "missing file is a usage error" 2 check "$EXAMPLES/valid/no-such-file.types"
expect_exit "no arguments is a usage error" 2 check
expect_exit "multiple valid files share a namespace" 0 check "$EXAMPLES/valid/shapes.types" "$EXAMPLES/valid/list.types"

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
if [[ "$loud" == OK:* ]]; then
    pass "-v prints a success message"
else
    fail "-v prints a success message (got '$loud')"
fi
echo ""

TOTAL=$(( PASS + FAIL ))
echo "Results: $PASS/$TOTAL passed"
if [[ $FAIL -gt 0 ]]; then
    echo "FAILED: $FAIL test(s) failed"
    exit 1
fi
