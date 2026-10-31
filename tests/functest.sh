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
# A hang would stop the whole run, so this one is given a time limit. macOS has no timeout command unless coreutils
# is installed, where it is called gtimeout, and without either the limit is simply not applied.
limit=()
if command -v timeout >/dev/null 2>&1; then
    limit=(timeout 20)
elif command -v gtimeout >/dev/null 2>&1; then
    limit=(gtimeout 20)
fi
actual=0
"${limit[@]+"${limit[@]}"}" "$CONSTRUCTO" check --allow-refs "$REFS/repeated-parameter.types" >/dev/null 2>&1 || actual=$?
if [[ "$actual" -eq 0 ]]; then
    pass "a parameter that is used twice in a type can be referred to twice"
else
    fail "a parameter that is used twice in a type can be referred to twice (exit $actual; 124 is a timeout)"
fi
# Only the first type error is reported, but every file that has one has failed.
loud="$("$CONSTRUCTO" check -v "$PBI/const-wrong-type.types" "$PBI/record-const-missing-field.types" 2>/dev/null || true)"
if [[ "$loud" == *"FAILED: "*"const-wrong-type.types"* && "$loud" == *"FAILED: "*"record-const-missing-field.types"* \
    && "$loud" != *"OK:"* && "$loud" != *"  const "* ]]; then
    pass "-v marks every file with a const error as failed, though only the first error is reported"
else
    fail "-v marks every file with a const error as failed, though only the first error is reported (got '$loud')"
fi
err="$("$CONSTRUCTO" check "$PBI/const-wrong-type.types" "$PBI/record-const-missing-field.types" 2>&1 >/dev/null || true)"
if [[ "$(wc -l <<< "$err")" -eq 1 ]]; then
    pass "only the first const error is reported without --continue"
else
    fail "only the first const error is reported without --continue (got '$err')"
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

echo "--- omitted arguments ---"
expect_exit "--default-limit can refuse a default" 1 check --default-limit=2 "$TYPED/schema.types" "$TYPED/valid/omitted-arguments.values"
expect_exit "--default-limit=N takes a space as well" 1 check --default-limit 2 "$TYPED/schema.types" "$TYPED/valid/omitted-arguments.values"
expect_exit "--default-limit needs a number" 2 check --default-limit=many "$TYPED/schema.types" "$TYPED/valid/omitted-arguments.values"
err="$("$CONSTRUCTO" check --default-limit=2 "$TYPED/schema.types" "$TYPED/valid/omitted-arguments.values" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: The default value of { cpu: Nat, mem: Nat } has more than 2 nodes, which is too large; give argument limits or raise --default-limit ("* ]]; then
    pass "the error for a large default names the limit and the option"
else
    fail "the error for a large default names the limit and the option (got '$err')"
fi
# Each type holds two copies of the one before it, so the size of its default doubles.
doubling="$(mktemp)"
trap 'rm -f "$chain" "$doubling"' EXIT
{
    echo "enum E0 = E0;"
    for i in $(seq 1 20); do echo "enum E$i = E$i(a: E$(( i - 1 )), b: E$(( i - 1 )));"; done
    echo "enum Holder = Holder(x: E20);"
} > "$doubling"
doubled_values="$(mktemp)"
trap 'rm -f "$chain" "$doubling" "$doubled_values"' EXIT
echo "Holder();" > "$doubled_values"
expect_exit "a huge default is refused" 1 check "$doubling" "$doubled_values"
expect_exit "--default-limit raises the limit" 0 check --default-limit=5000000 "$doubling" "$doubled_values"
# A chain of declarations that each start with the next must not overflow the stack, whatever the limit.
long_chain="$(mktemp)"
trap 'rm -f "$chain" "$doubling" "$doubled_values" "$long_chain"' EXIT
{
    echo "enum E0 = E0;"
    for i in $(seq 1 50000); do echo "enum E$i = E$i(a: E$(( i - 1 )));"; done
    echo "enum Holder = Holder(x: E50000);"
} > "$long_chain"
# The default of a type may need an enum that only a later types file declares, which the values cannot name.
forward_types="$(mktemp)"
forward_later="$(mktemp)"
forward_values="$(mktemp)"
trap 'rm -f "$chain" "$doubling" "$doubled_values" "$long_chain" "$forward_types" "$forward_later" "$forward_values"' EXIT
echo "enum Holder = Holder(x: Later);" > "$forward_types"
echo "enum Later = Ready;" > "$forward_later"
echo "Holder();" > "$forward_values"
expect_exit "the default may use an enum that is declared in a later types file" 0 \
    check "$forward_types" "$forward_later" "$forward_values"
err="$("$CONSTRUCTO" check "$forward_types" "$forward_values" "$forward_later" 2>&1 >/dev/null || true)"
if [[ "$err" == *"A types file must come before the values files, but this follows $forward_values ($forward_later, line 1, col 1)"* ]]; then
    pass "a types file after a values file is reported with the values file"
else
    fail "a types file after a values file is reported with the values file (got '$err')"
fi
expect_exit "a types file after a values file is an error" 1 check "$forward_types" "$forward_values" "$forward_later"
expect_exit "the order is still checked without the type-check" 1 \
    check --no-type-check "$forward_types" "$forward_values" "$forward_later"
expect_exit "several values files may follow the types files" 0 \
    check "$forward_types" "$forward_later" "$forward_values" "$forward_values"
expect_exit "several values files may come first when there are no types" 0 \
    check --no-prelude --no-type-check "$forward_values" "$forward_values"
expect_exit "a types file after a values file is an error whatever is given with it" 1 \
    check --stdin "$forward_values" < "$forward_types"
echo "// Only a comment." > "$forward_types"
expect_exit "a file with no declarations may follow a values file" 0 \
    check --no-type-check "$forward_values" "$forward_types"
expect_exit "a long chain of defaults is refused, not a crash" 1 check "$long_chain" "$doubled_values"
expect_exit "a long chain of defaults is accepted within the limit" 0 check --default-limit=1000000 "$long_chain" "$doubled_values"
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

echo "--- experimental features ---"
EXPERIMENTAL="$VALUES/experimental"
# Every example needs the feature, so each one fails without it, with a message that says how to switch it on.
for f in "$EXPERIMENTAL"/valid/*.values; do
    name="experimental/valid/$(basename "$f")"
    expect_exit "$name with -x" 0 check -x --no-type-check "$f"
    expect_exit "$name with --experimental=some-query" 0 check --experimental=some-query --no-type-check "$f"
    expect_exit "$name with -x=some-query" 0 check -x=some-query --no-type-check "$f"
    expect_exit "$name without -x" 1 check --no-type-check "$f"
done
for f in "$EXPERIMENTAL"/typed/valid/*.values; do
    name="experimental/typed/valid/$(basename "$f")"
    expect_exit "$name with -x" 0 check -x "$TYPED/schema.types" "$f"
    expect_exit "$name without -x" 1 check "$TYPED/schema.types" "$f"
done
for f in "$EXPERIMENTAL"/typed/invalid/*.values; do
    expected="$(sed -n 's|^// expect: ||p' "$f")"
    actual=0
    err="$("$CONSTRUCTO" check -x "$TYPED/schema.types" "$f" 2>&1 >/dev/null)" || actual=$?
    if [[ -n "$expected" && "$actual" -eq 1 && "$err" == "ERROR: $expected ("* ]]; then
        pass "experimental/typed/invalid/$(basename "$f")"
    else
        fail "experimental/typed/invalid/$(basename "$f") (expected '$expected' and exit 1, got exit $actual and '$err')"
    fi
done
err="$("$CONSTRUCTO" check "$EXPERIMENTAL/valid/some-query.values" --no-type-check 2>&1 >/dev/null || true)"
question="Postfix '?' is experimental; enable it with --experimental=some-query"
if [[ "$err" == "ERROR: $question ("*"line 2, col 2)" ]]; then
    pass "the message says how to switch the feature on"
else
    fail "the message says how to switch the feature on (got '$err')"
fi
expect_exit "a feature in a const of a types file needs the option too" 1 check \
    <(printf 'const a = 0?;\n')
expect_exit "a feature in a const of a types file with -x" 0 check -x <(printf 'const a = 0?;\n')
expect_exit "a bare -x does not take the file as its value" 0 \
    check -x --no-type-check "$EXPERIMENTAL/valid/some-query.values"
expect_exit "the option can be repeated" 0 check -x=some-query -x=all --no-type-check \
    "$EXPERIMENTAL/valid/some-query.values"
expect_exit "an unknown feature is a usage error" 2 check -x=nonsense "$EXAMPLES/valid/empty.types"
expect_exit "a name after a space is taken as a file, which cannot be read" 2 \
    check -x nonsense "$EXAMPLES/valid/empty.types"
err="$("$CONSTRUCTO" check -x=nonsense "$EXAMPLES/valid/empty.types" 2>&1 >/dev/null || true)"
if [[ "$err" == *"some-query"* ]]; then
    pass "an unknown feature is reported with the names that are known"
else
    fail "an unknown feature is reported with the names that are known (got '$err')"
fi
expect_exit "without the prelude the shorthands are unknown" 1 \
    check --no-prelude -x "$EXAMPLES/valid/postfix-optional.types"
echo ""

echo "--- paired flags ---"
# The last of a flag and its opposite wins, whichever order they come in.
TYPES="$EXAMPLES/valid/postfix-optional.types"
SOME_QUERY="$VALUES/experimental/valid/some-query.values"
expect_exit "--prelude after --no-prelude loads the prelude" 0 check --no-prelude --prelude "$TYPES"
expect_exit "--no-prelude after --prelude does not" 1 check --prelude --no-prelude "$TYPES"
expect_exit "--type-check after --no-type-check checks the types" 1 \
    check --no-type-check --type-check "$VALUES/valid/single-value.values"
expect_exit "--no-type-check after --type-check does not" 0 \
    check --type-check --no-type-check "$VALUES/valid/single-value.values"
expect_exit "--widening after --no-widening widens" 0 \
    check --no-widening --widening "$TYPED/schema.types" "$TYPED/valid/shapes.values"
expect_exit "--no-widening after --widening does not" 1 \
    check --widening --no-widening "$TYPED/schema.types" "$TYPED/valid/shapes.values"
for order in "--continue --no-continue" "--no-continue --continue"; do
    # shellcheck disable=SC2086
    errors="$("$CONSTRUCTO" check $order "$TYPED/schema.types" "$TYPED/invalid/missing-field.values" \
        "$TYPED/invalid/unknown-field.values" 2>&1 >/dev/null || true)"
    if [[ "$order" == "--continue --no-continue" && "$(wc -l <<< "$errors")" -eq 1 ]] \
        || [[ "$order" == "--no-continue --continue" && "$(wc -l <<< "$errors")" -eq 2 ]]; then
        pass "the last of $order wins"
    else
        fail "the last of $order wins (got '$errors')"
    fi
done
expect_exit "--allow-refs after --no-allow-refs allows references" 0 \
    check --no-allow-refs --allow-refs "$REFS/chain.types"
expect_exit "--no-allow-refs after --allow-refs does not" 1 check --allow-refs --no-allow-refs "$REFS/chain.types"
quiet="$("$CONSTRUCTO" check -v --no-verbose "$EXAMPLES/valid/empty.types" 2>&1)"
loud="$("$CONSTRUCTO" check --no-verbose -v "$EXAMPLES/valid/empty.types" 2>&1)"
if [[ -z "$quiet" && "$loud" == *"OK: checked 1 file(s)"* ]]; then
    pass "the last of --verbose and --no-verbose wins"
else
    fail "the last of --verbose and --no-verbose wins (got '$quiet' and '$loud')"
fi
expect_exit "-x then --no-experimental=some-query switches the feature off" 1 \
    check -x --no-experimental=some-query --no-type-check "$SOME_QUERY"
expect_exit "--no-experimental=some-query then -x switches it on" 0 \
    check --no-experimental=some-query -x --no-type-check "$SOME_QUERY"
expect_exit "-x then a bare --no-experimental switches every feature off" 1 \
    check -x --no-experimental --no-type-check "$SOME_QUERY"
expect_exit "a bare --no-experimental then -x=some-query switches it on" 0 \
    check --no-experimental -x=some-query --no-type-check "$SOME_QUERY"
expect_exit "--no-experimental=some-query on its own changes nothing" 1 \
    check --no-experimental=some-query --no-type-check "$SOME_QUERY"
expect_exit "an unknown name for --no-experimental is a usage error" 2 \
    check --no-experimental=nonsense "$EXAMPLES/valid/empty.types"
echo ""

echo "--- configuration files ---"
CONFIG="$(dirname "$0")/../syntax/examples/config"
EMPTY="$EXAMPLES/valid/empty.types"
for f in "$CONFIG"/valid/*.toml; do
    expect_exit "config/valid/$(basename "$f")" 0 check --config="$f" "$EMPTY"
    expect_exit "config/valid/$(basename "$f") with a space before the path" 0 check --config "$f" "$EMPTY"
done
# Each invalid file names its expected error, which is reported in the usual form with exit code 2.
for f in "$CONFIG"/invalid/*.toml; do
    expected="$(sed -n 's|^# expect: ||p' "$f")"
    actual=0
    err="$("$CONSTRUCTO" check --config="$f" "$EMPTY" 2>&1 >/dev/null)" || actual=$?
    if [[ -n "$expected" && "$actual" -eq 2 && "$err" == "ERROR: $expected"*"($f, line "* ]]; then
        pass "config/invalid/$(basename "$f")"
    else
        fail "config/invalid/$(basename "$f") (expected exit 2 with '$expected', got exit $actual and '$err')"
    fi
done
expect_exit "a missing config file is a usage error" 2 check --config=nothing-here.toml "$EMPTY"
err="$("$CONSTRUCTO" check --config=nothing-here.toml "$EMPTY" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: Cannot read nothing-here.toml: "* ]]; then
    pass "a missing config file is reported"
else
    fail "a missing config file is reported (got '$err')"
fi
expect_exit "--config can only be given once" 2 check --config="$CONFIG/valid/empty.toml" \
    --config="$CONFIG/valid/full.toml" "$EMPTY"
expect_exit "--config cannot be given before and after the subcommand" 2 --config="$CONFIG/valid/empty.toml" \
    check --config="$CONFIG/valid/full.toml" --print-config
expect_exit "--config cannot be given before and after the subcommand, with a space" 2 \
    --config "$CONFIG/valid/empty.toml" check --config "$CONFIG/valid/full.toml" "$EMPTY"
err="$("$CONSTRUCTO" --config="$CONFIG/valid/empty.toml" check --config="$CONFIG/valid/full.toml" "$EMPTY" 2>&1 || true)"
if [[ "$err" == *"--config can only be given once"* ]]; then
    pass "the repeated --config is reported"
else
    fail "the repeated --config is reported (got '$err')"
fi
expect_exit "--config comes before the subcommand too" 0 --config="$CONFIG/valid/full.toml" check "$EMPTY"
expect_exit "the config is read even if there are no files to check" 2 check --stdin \
    --config="$CONFIG/invalid/unknown-key.toml" < /dev/null
echo ""

echo "--- the command line overrides the config file ---"
PRE="$CONFIG/precedence"
two_errors() {
    # Prints the number of errors reported for the two ill-typed values.
    "$CONSTRUCTO" check "$@" "$TYPED/schema.types" "$TYPED/invalid/missing-field.values" \
        "$TYPED/invalid/unknown-field.values" 2>&1 >/dev/null | wc -l
}
if [[ "$(two_errors)" -eq 1 && "$(two_errors --config="$PRE/continue.toml")" -eq 2 \
    && "$(two_errors --config="$PRE/continue.toml" --no-continue)" -eq 1 \
    && "$(two_errors --no-continue --config="$PRE/continue.toml")" -eq 1 \
    && "$(two_errors --config="$PRE/continue.toml" --continue)" -eq 2 ]]; then
    pass "continue: default, file and argument"
else
    fail "continue: default, file and argument"
fi
expect_exit "prelude: the file turns it off" 1 check --config="$PRE/no-prelude.toml" "$TYPES"
expect_exit "prelude: --prelude overrides the file" 0 check --config="$PRE/no-prelude.toml" --prelude "$TYPES"
expect_exit "type-check: the file turns it off" 0 \
    check --config="$PRE/no-type-check.toml" "$VALUES/valid/single-value.values"
expect_exit "type-check: --type-check overrides the file" 1 \
    check --config="$PRE/no-type-check.toml" --type-check "$VALUES/valid/single-value.values"
expect_exit "widening: the file turns it off" 1 \
    check --config="$PRE/no-widening.toml" "$TYPED/schema.types" "$TYPED/valid/shapes.values"
expect_exit "widening: --widening overrides the file" 0 \
    check --config="$PRE/no-widening.toml" --widening "$TYPED/schema.types" "$TYPED/valid/shapes.values"
expect_exit "allow-refs: the file turns it on" 0 check --config="$PRE/allow-refs.toml" "$REFS/chain.types"
expect_exit "allow-refs: --no-allow-refs overrides the file" 1 \
    check --config="$PRE/allow-refs.toml" --no-allow-refs "$REFS/chain.types"
quiet="$("$CONSTRUCTO" check "$EMPTY" 2>&1)"
loud="$("$CONSTRUCTO" check --config="$PRE/verbose.toml" "$EMPTY" 2>&1)"
overridden="$("$CONSTRUCTO" check --config="$PRE/verbose.toml" --no-verbose "$EMPTY" 2>&1)"
if [[ -z "$quiet" && "$loud" == *"OK: checked 1 file(s)"* && -z "$overridden" ]]; then
    pass "verbose: default, file and argument"
else
    fail "verbose: default, file and argument (got '$quiet', '$loud' and '$overridden')"
fi
OMITTED="$TYPED/valid/omitted-arguments.values"
expect_exit "default-limit: the file sets it" 1 check --config="$PRE/limits.toml" "$TYPED/schema.types" "$OMITTED"
expect_exit "default-limit: the argument overrides the file" 0 \
    check --config="$PRE/limits.toml" --default-limit=10000 "$TYPED/schema.types" "$OMITTED"
err="$("$CONSTRUCTO" check --config="$PRE/limits.toml" --allow-refs "$chain" 2>&1 >/dev/null || true)"
if [[ "$err" == *"has more than 100 nodes"* ]]; then
    pass "infer-limit: the file sets it"
else
    fail "infer-limit: the file sets it (got '$err')"
fi
expect_exit "infer-limit: the argument overrides the file" 0 \
    check --config="$PRE/limits.toml" --infer-limit=5000000 --allow-refs "$chain"
expect_exit "some-query: the file switches the feature on" 0 \
    check --config="$PRE/some-query.toml" --no-type-check "$SOME_QUERY"
expect_exit "some-query: --no-experimental=some-query overrides the file" 1 \
    check --config="$PRE/some-query.toml" --no-experimental=some-query --no-type-check "$SOME_QUERY"
expect_exit "some-query: a bare --no-experimental overrides the file" 1 \
    check --config="$PRE/some-query.toml" --no-experimental --no-type-check "$SOME_QUERY"
expect_exit "some-query: -x after --no-experimental switches it on again" 0 \
    check --config="$PRE/some-query.toml" --no-experimental -x --no-type-check "$SOME_QUERY"
echo "--- projects ---"
PROJECTS="$(dirname "$0")/../syntax/examples/project"
SHAPES="$PROJECTS/valid/shapes"
for d in "$PROJECTS"/valid/*/; do
    d="${d%/}"
    expect_exit "project/valid/$(basename "$d")" 0 check --project="$d"
done
# Each invalid project-file names its expected error, which is reported in the usual form with exit code 2.
for d in "$PROJECTS"/invalid/*/; do
    d="${d%/}"
    [[ -f "$d/constructo.toml" ]] || continue
    expected="$(sed -n 's|^# expect: ||p' "$d/constructo.toml")"
    actual=0
    err="$("$CONSTRUCTO" check --project="$d" 2>&1 >/dev/null)" || actual=$?
    if [[ "$actual" -eq 2 && "$err" == "ERROR: $expected ($d/constructo.toml, line "* ]]; then
        pass "project/invalid/$(basename "$d")"
    else
        fail "project/invalid/$(basename "$d") (expected exit 2 with '$expected', got exit $actual and '$err')"
    fi
done
expect_exit "a folder with no project-file is a usage error" 2 check --project="$PROJECTS/invalid/no-project-file"
err="$("$CONSTRUCTO" check --project="$PROJECTS/invalid/no-project-file" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: Cannot read $PROJECTS/invalid/no-project-file/constructo.toml: "* ]]; then
    pass "a folder with no project-file is reported"
else
    fail "a folder with no project-file is reported (got '$err')"
fi
expect_exit "a folder that does not exist is a usage error" 2 check --project=nothing-here
expect_exit "--project can only be given once" 2 check --project="$SHAPES" --project="$SHAPES"
expect_exit "--project needs a folder" 2 check --project
expect_exit "--project is not accepted without a subcommand" 2 --project="$SHAPES"

# The files of the project are the types files that its patterns name, in order, and nothing else.
listing="$("$CONSTRUCTO" check -v --project="$SHAPES" 2>&1)"
expected_listing="OK: $SHAPES (project com.example.shapes)
OK: $SHAPES/circles/shape.types (data-types)
OK: $SHAPES/root.types (data-types)
OK: checked 2 file(s)"
if [[ "$listing" == "$expected_listing" ]]; then
    pass "project: the types files are listed in order, without hidden, nested or other files"
else
    fail "project: the types files are listed in order (got '$listing')"
fi
listing="$("$CONSTRUCTO" check -v --project="$PROJECTS/valid/patterns" 2>&1)"
if [[ "$listing" == *"/lib/b.types"* && "$listing" == *"/src/a.types"* && "$listing" != *"generated"* \
    && "$listing" != *"other.types"* ]]; then
    pass "project: include and exclude choose the files"
else
    fail "project: include and exclude choose the files (got '$listing')"
fi
listing="$("$CONSTRUCTO" check -v --project="$PROJECTS/valid/imports-only" 2>&1)"
if [[ "$listing" == "OK: $PROJECTS/valid/imports-only (project example.importsonly)
OK: checked 0 file(s)" ]]; then
    pass "project: a project with no types files is accepted"
else
    fail "project: a project with no types files is accepted (got '$listing')"
fi

# A file that the project includes as a types file must not hold values.
err="$("$CONSTRUCTO" check --project="$PROJECTS/rejected/values-in-types" 2>&1 >/dev/null)" && actual=0 || actual=$?
if [[ "$actual" -eq 1 && "$err" == "ERROR: The project includes this file as a types file but it holds data-values ($PROJECTS/rejected/values-in-types/notes.types, line 1, col 1)" ]]; then
    pass "project: a values file that the patterns include is reported"
else
    fail "project: a values file that the patterns include is reported (got exit $actual and '$err')"
fi

# Imports: every project that is imported is loaded once, before the project that imports it.
IMPORTS="$PROJECTS/valid-imports"
for root in diamond/app two-groups/app chain/a; do
    expect_exit "project/valid-imports/$root" 0 check --project="$IMPORTS/$root"
done
project_names() { "$CONSTRUCTO" check -v --project="$1" 2>&1 | sed -n 's|.*(project \(.*\))$|\1|p' | tr '\n' ' '; }
order="$(project_names "$IMPORTS/diamond/app")"
if [[ "$order" == "example.base example.left example.right example.app " ]]; then
    pass "imports: a project that is imported twice is loaded once, before the projects that import it"
else
    fail "imports: a project that is imported twice is loaded once, before the projects that import it (got '$order')"
fi
order="$(project_names "$IMPORTS/chain/a")"
if [[ "$order" == "example.c example.b example.a " ]]; then
    pass "imports: a project is loaded when it is imported indirectly"
else
    fail "imports: a project is loaded when it is imported indirectly (got '$order')"
fi
count="$("$CONSTRUCTO" check -v --project="$IMPORTS/two-groups/app" 2>&1 | grep -c '(project ')"
if [[ "$count" -eq 2 ]]; then
    pass "imports: one project in several groups is still loaded once"
else
    fail "imports: one project in several groups is still loaded once (got $count)"
fi
# A relative source is relative to the folder of the project that imports, and not to the current folder.
if (cd "$IMPORTS/diamond" && "$CONSTRUCTO" check --project=app >/dev/null 2>&1); then
    pass "imports: a relative source is relative to the importing project"
else
    fail "imports: a relative source is relative to the importing project"
fi
absolute="$(mktemp -d)"
printf '[project]\nname = "example.absolute"\n\n[[import]]\nproject-name = "example.lib"\nsource = "%s"\n' \
    "$(cd "$IMPORTS/two-groups/lib" && pwd)" > "$absolute/constructo.toml"
expect_exit "imports: a source may be an absolute path" 0 check --project="$absolute"
rm -f "$absolute/constructo.toml"
rmdir "$absolute"
# An error in an imported project is reported against that project.
broken="$(mktemp -d)"
mkdir "$broken/lib" "$broken/app"
printf '[project]\nname = "example.lib"\n' > "$broken/lib/constructo.toml"
echo "enum Lib = Lib(Missing);" > "$broken/lib/lib.types"
printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.lib"\nsource = "../lib"\n' \
    > "$broken/app/constructo.toml"
echo "enum App = App;" > "$broken/app/app.types"
expect_exit "imports: a type error in an imported project fails the run" 1 check --project="$broken/app"
listing="$("$CONSTRUCTO" check -v --project="$broken/app" 2>/dev/null || true)"
if [[ "$listing" == *"FAILED: $broken/app/../lib (project example.lib)"* \
    && "$listing" == *"OK: $broken/app (project example.app)"* ]]; then
    pass "imports: the project with the error is the one reported as failed"
else
    fail "imports: the project with the error is the one reported as failed (got '$listing')"
fi
rm -f "$broken/lib/constructo.toml" "$broken/lib/lib.types" "$broken/app/constructo.toml" "$broken/app/app.types"
rmdir "$broken/lib" "$broken/app" "$broken"
# Each invalid import names its expected error, which starts the message, and the exit code is 2.
for d in "$PROJECTS"/invalid-imports/*/; do
    d="${d%/}"
    expected="$(sed -n 's|^# expect: ||p' "$d/app/constructo.toml")"
    actual=0
    err="$("$CONSTRUCTO" check --project="$d/app" 2>&1 >/dev/null)" || actual=$?
    if [[ "$actual" -eq 2 && "$err" == "ERROR: $expected"* ]]; then
        pass "project/invalid-imports/$(basename "$d")"
    else
        fail "project/invalid-imports/$(basename "$d") (expected exit 2 with '$expected', got exit $actual and '$err')"
    fi
done
# Names: each project has its own names, which are written with a group or the full name of the project.
NAMES="$PROJECTS/valid-names"
expect_exit "project/valid-names/qualified" 0 check --project="$NAMES/qualified/app" "$NAMES/qualified/app/values.values"
for case in bundle same-names group; do
    expect_exit "project/valid-names/$case" 0 check --project="$NAMES/$case/app"
done
# A const of another project is a reference to a const, which needs the option as any other does.
expect_exit "project/valid-names/not-transitive" 0 check --allow-refs --project="$NAMES/not-transitive/app"
expect_exit "a const of another project is a reference" 1 check --project="$NAMES/not-transitive/app"
# Each invalid case names its expected error, which starts the message, and the exit code is 1.
for d in "$PROJECTS"/invalid-names/*/; do
    d="${d%/}"
    expected="$(sed -n '1s|^// expect: ||p' "$d/app/app.types")"
    actual=0
    err="$("$CONSTRUCTO" check --project="$d/app" 2>&1 >/dev/null)" || actual=$?
    if [[ "$actual" -eq 1 && "$err" == "ERROR: $expected"* ]]; then
        pass "project/invalid-names/$(basename "$d")"
    else
        fail "project/invalid-names/$(basename "$d") (expected exit 1 with '$expected', got exit $actual and '$err')"
    fi
done
# A name is shown as it would be written in the file the message is about.
listing="$("$CONSTRUCTO" check -v --project="$NAMES/qualified/app" 2>/dev/null)"
if [[ "$listing" == *"const unit: example.lib::Shape"* && "$listing" == *"const holder: Holder"* ]]; then
    pass "names: a type of another project is shown in full, and one of the project itself is bare"
else
    fail "names: a type of another project is shown in full, and one of the project itself is bare (got '$listing')"
fi
listing="$("$CONSTRUCTO" check -v --project="$NAMES/bundle/app" 2>/dev/null)"
if [[ "$listing" == *"const c: Shape"* && "$listing" != *"example.lib::"* ]]; then
    pass "names: a type of a project that is imported into local is bare"
else
    fail "names: a type of a project that is imported into local is bare (got '$listing')"
fi
# The files given with a project are written in the project's own namespace, with its own imports.
extra="$(mktemp)"
echo "enum Extra = Extra(lib::Shape);" > "$extra"
expect_exit "names: a types file that is given is written in the project" 0 check --project="$NAMES/qualified/app" "$extra"
echo "enum Holder = Other;" > "$extra"
expect_exit "names: a type in a given file that the project declares is a duplicate" 1 \
    check --project="$NAMES/qualified/app" "$extra"
echo "enum Extra = Extra(example.other::Shape);" > "$extra"
expect_exit "names: a given file cannot name a project that was not imported" 1 \
    check --project="$NAMES/qualified/app" "$extra"
rm -f "$extra"
# The values to check come after the project, and a failed project is reported as such.
expect_exit "project: values are type-checked against the project" 0 check --project="$SHAPES" "$SHAPES/values/shapes.values"
echo "Circle { radius: \"wide\" }" > "$forward_values"
expect_exit "project: a type error in the values fails" 1 check --project="$SHAPES" "$forward_values"
expect_exit "project: --stdin is checked against the project" 0 check --project="$SHAPES" --stdin < "$SHAPES/values/shapes.values"
expect_exit "project: more types files may follow on the command line" 0 \
    check --project="$SHAPES" "$EXAMPLES/valid/message.types" "$SHAPES/values/shapes.values"
expect_exit "project: but not after a values file" 1 \
    check --project="$SHAPES" "$SHAPES/values/shapes.values" "$EXAMPLES/valid/message.types"
expect_exit "project: --no-type-check applies" 0 check --no-type-check --project="$SHAPES" "$forward_values"
verbose="$("$CONSTRUCTO" check -v --project="$SHAPES" "$forward_values" 2>/dev/null || true)"
verbose="${verbose%%$'\n'*}"
if [[ "$verbose" == "OK: $SHAPES (project com.example.shapes)" ]]; then
    pass "project: verbose mode reports the project"
else
    fail "project: verbose mode reports the project (got '$verbose')"
fi
# The prelude is added ahead of the project as for plain files, so a project that declares Bool needs --no-prelude.
bool_project="$(mktemp -d)"
printf '[project]\nname = "example.ownbool"\n' > "$bool_project/constructo.toml"
echo "enum Bool = No, Yes;" > "$bool_project/bool.types"
expect_exit "project: its own Bool clashes with the prelude" 1 check --project="$bool_project"
expect_exit "project: its own Bool is fine with --no-prelude" 0 check --no-prelude --project="$bool_project"
# Settings come from --config and the arguments only, never from the project-file.
expect_exit "project: --config still applies" 0 check --config="$CONFIG/precedence/no-prelude.toml" --project="$bool_project"
expect_exit "project: --prelude overrides --config" 1 \
    check --config="$CONFIG/precedence/no-prelude.toml" --prelude --project="$bool_project"
verbose="$("$CONSTRUCTO" check -v --project="$bool_project" 2>/dev/null || true)"
verbose="${verbose%%$'\n'*}"
if [[ "$verbose" == "FAILED: $bool_project (project example.ownbool)" ]]; then
    pass "project: a project whose file fails is reported as failed"
else
    fail "project: a project whose file fails is reported as failed (got '$verbose')"
fi
rm -f "$bool_project/constructo.toml" "$bool_project/bool.types"
rmdir "$bool_project"
printed="$("$CONSTRUCTO" check --print-config --project="$SHAPES")"
if [[ "$printed" == "$("$CONSTRUCTO" check --print-config)" ]]; then
    pass "project: the project-file does not change the settings"
else
    fail "project: the project-file does not change the settings"
fi
echo ""

echo "--- printing the effective settings ---"
printed() { "$CONSTRUCTO" check --print-config "$@"; }
# True if the printed settings contain the line, which is a key and its value.
prints() { local line="$1"; shift; printed "$@" | grep -qxF "$line"; }
expect_exit "--print-config needs no files" 0 check --print-config
expect_exit "--print-config does not check the files" 0 check --print-config "$TYPED/schema.types" "$TYPED/invalid/missing-field.values"
if [[ -z "$(printed 2>&1 >/dev/null)" && "$(printed)" == "$(printed --config="$CONFIG/valid/full.toml")" \
    && "$(printed)" == "$(printed --config="$CONFIG/valid/empty.toml")" ]]; then
    pass "print-config: the defaults are what the file with every default key gives"
else
    fail "print-config: the defaults are what the file with every default key gives"
fi
for line in "verbose = false" "prelude = true" "type-check = true" "widening = true" "continue = false" \
    "allow-refs = false" "infer-limit = 10000" "default-limit = 10000" "some-query = false"; do
    if prints "$line"; then pass "print-config shows $line by default"; else fail "print-config shows $line by default"; fi
done
for check in "continue = true:continue" "prelude = false:no-prelude" "type-check = false:no-type-check" \
    "widening = false:no-widening" "allow-refs = true:allow-refs" "verbose = true:verbose" \
    "infer-limit = 100:limits" "default-limit = 2:limits" "some-query = true:some-query"; do
    line="${check%%:*}"; file="${check##*:}"
    if prints "$line" --config="$PRE/$file.toml"; then
        pass "print-config: $file.toml gives $line"
    else
        fail "print-config: $file.toml gives $line"
    fi
done
if prints "continue = false" --config="$PRE/continue.toml" --no-continue \
    && prints "continue = true" --no-continue --continue \
    && prints "prelude = true" --config="$PRE/no-prelude.toml" --prelude \
    && prints "infer-limit = 7" --config="$PRE/limits.toml" --infer-limit=7 \
    && prints "some-query = false" --config="$PRE/some-query.toml" --no-experimental=some-query \
    && prints "some-query = true" --config="$PRE/some-query.toml" --no-experimental -x; then
    pass "print-config: an argument overrides the file"
else
    fail "print-config: an argument overrides the file"
fi
if printed | grep -qxF "# Load the standard prelude, with Bool and Option." \
    && [[ "$(printed | grep -c '^# ')" -eq 9 ]]; then
    pass "print-config: every key has a comment"
else
    fail "print-config: every key has a comment"
fi
help="$("$CONSTRUCTO" check --help)"
if [[ "$help" == *"--print-config > constructo-settings.toml"* && "$help" == *"docs/specs/config-file.md"* ]]; then
    pass "--help says how to write a config file"
else
    fail "--help says how to write a config file"
fi
reloadable="$(mktemp)"
printed --config="$PRE/limits.toml" --no-widening --no-prelude --continue --allow-refs -v -x > "$reloadable"
if [[ "$(printed --config="$reloadable")" == "$(cat "$reloadable")" ]]; then
    pass "print-config: the output can be loaded with --config"
else
    fail "print-config: the output can be loaded with --config"
fi
rm -f "$reloadable"
echo ""

TOTAL=$(( PASS + FAIL ))
echo "Results: $PASS/$TOTAL passed"
if [[ $FAIL -gt 0 ]]; then
    echo "FAILED: $FAIL test(s) failed"
    exit 1
fi
