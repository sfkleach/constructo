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

# These examples are not checked against any types, so only the well-formed check applies.
echo "--- valid values ---"
for f in "$VALUES"/valid/*.values; do
    expect_exit "values/valid/$(basename "$f")" 0 check --no-type-check "$f"
done
echo ""

echo "--- invalid values ---"
for f in "$VALUES"/invalid/*.values; do
    expect_exit "values/invalid/$(basename "$f")" 1 check --no-type-check "$f"
done
echo ""

TYPED="$VALUES/typed"

echo "--- well-typed values ---"
for f in "$TYPED"/valid/*.values; do
    expect_exit "typed/valid/$(basename "$f")" 0 check "$TYPED/schema.types" "$f"
done
echo ""

echo "--- ill-typed values ---"
# Each file names its expected error in a "// expect: ..." line, so that it cannot fail for some other reason.
for f in "$TYPED"/invalid/*.values; do
    expected="$(sed -n 's|^// expect: ||p' "$f")"
    actual=0
    err="$("$CONSTRUCTO" check "$TYPED/schema.types" "$f" 2>&1 >/dev/null)" || actual=$?
    if [[ -n "$expected" && "$actual" -eq 1 && "$err" == "ERROR: $expected ("* ]]; then
        pass "typed/invalid/$(basename "$f")"
    else
        fail "typed/invalid/$(basename "$f") (expected exit 1 with '$expected', got exit $actual and '$err')"
    fi
    expect_exit "typed/invalid/$(basename "$f") with --no-type-check" 0 check --no-type-check "$f"
done
echo ""

echo "--- type-check options ---"
expect_exit "types must come before the values that use them" 1 check "$TYPED/valid/shapes.values" "$TYPED/schema.types"
expect_exit "values without any types fail the type-check" 1 check "$VALUES/valid/single-value.values"
expect_exit "--no-widening rejects a Nat as a Float" 1 \
    check --no-widening "$TYPED/schema.types" "$TYPED/valid/shapes.values"
two_errors="$("$CONSTRUCTO" check "$TYPED/schema.types" "$TYPED/invalid/missing-field.values" \
    "$TYPED/invalid/unknown-field.values" 2>&1 >/dev/null || true)"
all_errors="$("$CONSTRUCTO" check --continue "$TYPED/schema.types" "$TYPED/invalid/missing-field.values" \
    "$TYPED/invalid/unknown-field.values" 2>&1 >/dev/null || true)"
if [[ "$(wc -l <<< "$two_errors")" -eq 1 && "$(wc -l <<< "$all_errors")" -eq 2 ]]; then
    pass "a type error stops the run unless --continue is given"
else
    fail "a type error stops the run unless --continue is given (got '$two_errors' and '$all_errors')"
fi
if [[ "$two_errors" == "ERROR: Missing field height in Box ("*"missing-field.values, line 2, col 1)" ]]; then
    pass "type error format"
else
    fail "type error format (got '$two_errors')"
fi
loud="$("$CONSTRUCTO" check -v "$TYPED/schema.types" "$TYPED/valid/shapes.values" 2>&1)"
if [[ "$loud" == *"shapes.values (data-values, type-checked)"* ]]; then
    pass "-v shows that values were type-checked"
else
    fail "-v shows that values were type-checked (got '$loud')"
fi
loud="$("$CONSTRUCTO" check -v --no-type-check "$TYPED/valid/shapes.values" 2>&1)"
if [[ "$loud" == *"shapes.values (data-values)"* && "$loud" != *"type-checked"* ]]; then
    pass "-v does not claim a type-check under --no-type-check"
else
    fail "-v does not claim a type-check under --no-type-check (got '$loud')"
fi
echo ""

echo "--- mixed types and values ---"
expect_exit "types and values together" 0 \
    check --no-type-check "$EXAMPLES/valid/shapes.types" "$VALUES/valid/tagged.values"
expect_exit "values need not match the types under --no-type-check" 0 \
    check --no-type-check "$EXAMPLES/valid/message.types" "$VALUES/valid/untagged.values"
expect_exit "a values syntax error after types exits 1" 1 \
    check "$EXAMPLES/valid/shapes.types" "$VALUES/invalid/missing-separator.values"
expect_exit "a values file needs no types file under --no-type-check" 0 \
    check --no-type-check "$VALUES/valid/single-value.values"
expect_exit "stdin may hold values" 0 check --no-type-check --stdin < "$VALUES/valid/single-value.values"
echo ""

echo "--- consts ---"
REFS="$EXAMPLES/refs"
expect_exit "a const may refer to another with --allow-refs" 0 check --allow-refs "$REFS/chain.types"
expect_exit "a const may not refer to another without --allow-refs" 1 check "$REFS/chain.types"
expect_exit "a circular definition is an error with --allow-refs" 1 check --allow-refs "$REFS/cycle.types"
err="$("$CONSTRUCTO" check --allow-refs "$REFS/cycle.types" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: Circular const definition A -> B -> A ("*"cycle.types, line 3, col 16)" ]]; then
    pass "a circular definition names the consts on the circle"
else
    fail "a circular definition names the consts on the circle (got '$err')"
fi
err="$("$CONSTRUCTO" check "$EXAMPLES/parses-but-invalid/const-reference-needs-flag.types" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: Reference to const Base in const Total requires --allow-refs ("*"line 3, col 20)" ]]; then
    pass "the error for a const reference names the flag"
else
    fail "the error for a const reference names the flag (got '$err')"
fi
expect_exit "generic consts are valid, and may use the prelude consts without --allow-refs" 0 \
    check "$EXAMPLES/valid/generic-consts.types"
expect_exit "a prelude const cannot be redefined" 1 check "$EXAMPLES/parses-but-invalid/redefines-null.types"
expect_exit "--no-prelude allows redefining a prelude const" 0 \
    check --no-prelude "$EXAMPLES/parses-but-invalid/redefines-null.types"
expect_exit "values may use null, true and false" 0 \
    check "$TYPED/schema.types" "$TYPED/valid/prelude-consts.values"
err="$("$CONSTRUCTO" check "$EXAMPLES/parses-but-invalid/generic-const-wrong-value.types" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: Expected T but found Nat ("*"line 2, col 32)" ]]; then
    pass "the value of a generic const is checked against unknown parameter types"
else
    fail "the value of a generic const is checked against unknown parameter types (got '$err')"
fi
PBI="$EXAMPLES/parses-but-invalid"
record_error() {
    local file="$1" expected="$2" err
    err="$("$CONSTRUCTO" check "$PBI/$file" 2>&1 >/dev/null || true)"
    if [[ "$err" == "ERROR: $expected ("* ]]; then
        pass "$file reports '$expected'"
    else
        fail "$file reports '$expected' (got '$err')"
    fi
}
record_error record-const-extra-field.types "Unknown field b in record"
record_error record-const-missing-field.types "Missing field b in record"
record_error record-const-wrong-field-type.types "Expected Nat but found String"
record_error record-const-tagged-value.types "Expected { x: Nat } but found Point of type Shape"
record_error record-const-for-enum.types "Expected Shape but found a record"
record_error record-type-duplicate-field.types "Duplicate field a"
expect_exit "consts without a type are inferred" 0 check "$EXAMPLES/valid/inferred-consts.types"
expect_exit "inferred consts may refer to one another with --allow-refs" 0 \
    check --allow-refs "$REFS/inferred-chain.types"
expect_exit "inferred consts need --allow-refs to refer to one another" 1 check "$REFS/inferred-chain.types"
err="$("$CONSTRUCTO" check --allow-refs "$REFS/inferred-wrong-use.types" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: Expected String but found const count of type Nat ("*"line 3, col 23)" ]]; then
    pass "a reference has the inferred type of its const"
else
    fail "a reference has the inferred type of its const (got '$err')"
fi
err="$("$CONSTRUCTO" check "$PBI/inferred-const-params.types" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: Type parameters of const none need a type ("*"line 2, col 12)" ]]; then
    pass "type parameters need a type"
else
    fail "type parameters need a type (got '$err')"
fi
loud="$("$CONSTRUCTO" check -v "$EXAMPLES/valid/inferred-consts.types" 2>&1)"
if [[ "$loud" == *"  const count: Nat (inferred)"* && "$loud" == *"  const mixed: [Float] (inferred)"* \
    && "$loud" == *"  const nothing<T>: [T] (inferred)"* && "$loud" == *"  const missing<T>: Option<T> (inferred)"* ]]; then
    pass "-v lists the consts with their inferred types"
else
    fail "-v lists the consts with their inferred types (got '$loud')"
fi
loud="$("$CONSTRUCTO" check -v "$EXAMPLES/valid/consts.types" 2>&1)"
if [[ "$loud" == *"  const Name: String"$'\n'* && "$loud" != *"Name: String (inferred)"* ]]; then
    pass "-v lists a declared type without marking it"
else
    fail "-v lists a declared type without marking it (got '$loud')"
fi
loud="$("$CONSTRUCTO" check -v --no-type-check "$EXAMPLES/valid/inferred-consts.types" 2>&1)"
if [[ "$loud" != *"  const "* ]]; then
    pass "-v lists no consts when the types are not checked"
else
    fail "-v lists no consts when the types are not checked (got '$loud')"
fi
# Each const of the chain holds two copies of the one before it, so the size of its type doubles.
chain="$(mktemp)"
trap 'rm -f "$chain"' EXIT
{
    echo "const x0 = { v: 1 };"
    for i in $(seq 1 20); do echo "const x$i = { a: x$(( i - 1 )), b: x$(( i - 1 )) };"; done
} > "$chain"
expect_exit "a huge inferred type is refused" 1 check --allow-refs "$chain"
expect_exit "--infer-limit raises the limit" 0 check --allow-refs --infer-limit=5000000 "$chain"
expect_exit "--infer-limit=N takes a space as well" 0 check --allow-refs --infer-limit 5000000 "$chain"
err="$("$CONSTRUCTO" check --allow-refs --infer-limit=100 "$chain" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: The type of const "*" has more than 100 nodes, which is too large to infer; give it a type or raise --infer-limit ("* ]]; then
    pass "the error for a large type names the limit and the option"
else
    fail "the error for a large type names the limit and the option (got '$err')"
fi
expect_exit "--infer-limit needs a number" 2 check --infer-limit=many "$chain"
# A hang would stop the whole run, so this one is given a time limit.
actual=0
timeout 20 "$CONSTRUCTO" check --allow-refs "$REFS/repeated-parameter.types" >/dev/null 2>&1 || actual=$?
if [[ "$actual" -eq 0 ]]; then
    pass "a parameter that is used twice in a type can be referred to twice"
else
    fail "a parameter that is used twice in a type can be referred to twice (exit $actual; 124 is a timeout)"
fi
expect_exit "an ill-typed const passes --no-type-check" 0 \
    check --no-type-check "$EXAMPLES/parses-but-invalid/const-wrong-type.types"
loud="$("$CONSTRUCTO" check -v "$EXAMPLES/valid/starts-with-const.types" 2>&1)"
if [[ "$loud" == *"starts-with-const.types (data-types)"* ]]; then
    pass "a file that starts with const is a data-types file"
else
    fail "a file that starts with const is a data-types file (got '$loud')"
fi
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
stdin_exit "--no-widening accepts exact numbers" 0 'Pair(1, -2); Circle { radius: 3.0 }' \
    --no-widening "$TYPED/schema.types"
stdin_exit "--no-widening rejects a Nat as an Int" 1 'Pair(1, 2)' --no-widening "$TYPED/schema.types"
stdin_exit "--no-widening rejects a join of numbers" 1 '[1, 2.5]' --no-widening
stdin_exit "widening accepts a join of numbers" 0 '[1, 2.5]'
# The prelude consts are ordinary consts, so they are only known when the prelude is loaded.
stdin_exit "null is accepted with the prelude" 0 'null'
stdin_exit "true and false are accepted with the prelude" 0 '[true, false]; Some(null)'
stdin_exit "null needs the prelude" 1 'null' --no-prelude
stdin_exit "true needs the prelude" 1 'true' --no-prelude
err="$(printf 'null\n' | "$CONSTRUCTO" check --stdin --no-prelude 2>&1 || true)"
if [[ "$err" == "ERROR: Unknown const null (<stdin>, line 1, col 1)" ]]; then
    pass "null without the prelude reports an unknown const"
else
    fail "null without the prelude reports an unknown const (got '$err')"
fi
stdin_exit "--no-prelude allows redefining null" 0 'const null: Nat = 0;' --no-prelude
stdin_exit "a generic const can be declared and used" 0 \
    'enum O<T> = N, S(T); const none<T>: O<T> = N; const a: O<Nat> = none; const b: O<String> = none;' --no-prelude --allow-refs
stdin_exit "a generic const that is used without --allow-refs fails" 1 \
    'enum O<T> = N, S(T); const none<T>: O<T> = N; const a: O<Nat> = none;' --no-prelude

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
loud="$("$CONSTRUCTO" check -v --no-type-check "$EXAMPLES/valid/shapes.types" "$VALUES/valid/tagged.values" 2>&1)"
if [[ "$loud" == *"OK: $EXAMPLES/valid/shapes.types (data-types)"* \
    && "$loud" == *"OK: $VALUES/valid/tagged.values (data-values)"* ]]; then
    pass "-v shows the kind of each file"
else
    fail "-v shows the kind of each file (got '$loud')"
fi
loud="$("$CONSTRUCTO" check -v --no-type-check "$VALUES/invalid/missing-separator.values" 2>&1 || true)"
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
