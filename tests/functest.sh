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
# Some tests change directory, so a relative path to the binary is made absolute. A bare command name is left alone.
if [[ "$CONSTRUCTO" == */* ]]; then
    CONSTRUCTO="$(cd "$(dirname "$CONSTRUCTO")" && pwd)/$(basename "$CONSTRUCTO")"
fi
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
if [[ "$err" == "ERROR: Circular const definition a -> b -> a ("*"cycle.types, line 3, col 16)" ]]; then
    pass "a circular definition names the consts on the circle"
else
    fail "a circular definition names the consts on the circle (got '$err')"
fi
err="$("$CONSTRUCTO" check "$EXAMPLES/parses-but-invalid/const-reference-needs-flag.types" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: Reference to const base in const total requires --allow-refs ("*"line 3, col 20)" ]]; then
    pass "the error for a const reference names the flag"
else
    fail "the error for a const reference names the flag (got '$err')"
fi
expect_exit "generic consts are valid, and may use the prelude consts without --allow-refs" 0 \
    check "$EXAMPLES/valid/generic-consts.types"
expect_exit "a prelude const that is redefined is a duplicate" 1 check "$EXAMPLES/parses-but-invalid/redefines-null.types"
expect_exit "--no-prelude allows redefining a prelude const" 0 \
    check --no-prelude --allow-refs "$EXAMPLES/parses-but-invalid/redefines-null.types"
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
if [[ "$loud" == *"  const name: String"$'\n'* && "$loud" != *"name: String (inferred)"* ]]; then
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
expect_exit "a prelude name that is redeclared is a duplicate" 1 check "$EXAMPLES/parses-but-invalid/redeclares-bool.types"
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
# A name that cannot be resolved in one value must not hide a type error in another, and the usual rules for stopping
# at the first error and for --continue apply to both kinds of error.
mixed='Circle { radius: "bad" }; missing::Circle; other::Circle'
stdin_lines() { printf '%s\n' "$mixed" | "$CONSTRUCTO" check --stdin "$@" "$TYPED/schema.types" 2>&1 >/dev/null | wc -l || true; }
if [[ "$(stdin_lines)" -eq 1 && "$(stdin_lines --continue)" -eq 3 ]]; then
    pass "name errors and type errors in values are reported in turn, one at a time unless --continue"
else
    fail "name errors and type errors in values are reported in turn (got $(stdin_lines) and $(stdin_lines --continue))"
fi
first="$(printf '%s\n' "$mixed" | "$CONSTRUCTO" check --stdin "$TYPED/schema.types" 2>&1 >/dev/null || true)"
if [[ "$first" == "ERROR: Expected Float but found String (<stdin>, line 1, col 18)" ]]; then
    pass "a type error before a name error is the one reported"
else
    fail "a type error before a name error is the one reported (got '$first')"
fi
# A file name that looks like a canonical name, as in `Shape@x.y.types`, is quoted as it is in a message.
at_dir="$(mktemp -d)"
printf 'enum Shape = Circle;\n' > "$at_dir/Shape@x.y.types"
printf 'enum Shape = Square;\n' > "$at_dir/two.types"
at_out="$("$CONSTRUCTO" check "$at_dir/Shape@x.y.types" "$at_dir/two.types" 2>&1 || true)"
if [[ "$at_out" == *"first declared at $at_dir/Shape@x.y.types, line 1, col 6"* ]]; then
    pass "a file name with an @ is not rewritten as a name"
else
    fail "a file name with an @ is not rewritten as a name (got '$at_out')"
fi
rm -f "$at_dir/Shape@x.y.types" "$at_dir/two.types"
rmdir "$at_dir"
# A byte order mark at the start of a file is removed, as rustc does, so an editor that adds one does no harm.
bom_dir="$(mktemp -d)"
printf '\xEF\xBB\xBFenum Shape = Circle { radius: Float };\n' > "$bom_dir/shape.types"
printf '\xEF\xBB\xBFCircle { radius: 1.0 }\n' > "$bom_dir/circle.values"
printf '\xEF\xBB\xBF[project]\nname = "a.b"\n' > "$bom_dir/constructo.toml"
expect_exit "a byte order mark is removed from a types file and a values file" 0 check \
    "$bom_dir/shape.types" "$bom_dir/circle.values"
expect_exit "a byte order mark is removed from a project file" 0 check --dev --project="$bom_dir"
rm -f "$bom_dir/shape.types" "$bom_dir/circle.values" "$bom_dir/constructo.toml"
rmdir "$bom_dir"
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
    expect_exit "project/valid/$(basename "$d")" 0 check --dev --project="$d"
done
# Each invalid project-file names its expected error, which is reported in the usual form with exit code 2.
for d in "$PROJECTS"/invalid/*/; do
    d="${d%/}"
    [[ -f "$d/constructo.toml" ]] || continue
    expected="$(sed -n 's|^# expect: ||p' "$d/constructo.toml")"
    actual=0
    err="$("$CONSTRUCTO" check --dev --project="$d" 2>&1 >/dev/null)" || actual=$?
    if [[ "$actual" -eq 2 && "$err" == "ERROR: $expected ($d/constructo.toml, line "* ]]; then
        pass "project/invalid/$(basename "$d")"
    else
        fail "project/invalid/$(basename "$d") (expected exit 2 with '$expected', got exit $actual and '$err')"
    fi
done
expect_exit "a folder with no project-file is a usage error" 2 check --dev --project="$PROJECTS/invalid/no-project-file"
err="$("$CONSTRUCTO" check --dev --project="$PROJECTS/invalid/no-project-file" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: Cannot read $PROJECTS/invalid/no-project-file/constructo.toml: "* ]]; then
    pass "a folder with no project-file is reported"
else
    fail "a folder with no project-file is reported (got '$err')"
fi
expect_exit "a folder that does not exist is a usage error" 2 check --dev --project=nothing-here
expect_exit "--project can only be given once" 2 check --dev --project="$SHAPES" --project="$SHAPES"
expect_exit "--project needs a folder" 2 check --project
expect_exit "--project is not accepted without a subcommand" 2 --project="$SHAPES"

# The files of the project are the types files that its patterns name, in order, and nothing else.
listing="$("$CONSTRUCTO" check -v --dev --project="$SHAPES" 2>&1)"
expected_listing="OK: $SHAPES (project com.example.shapes)
OK: $SHAPES/circles/shape.types (data-types)
OK: $SHAPES/root.types (data-types)
OK: checked 2 file(s) in 1 project(s)"
if [[ "$listing" == "$expected_listing" ]]; then
    pass "project: the types files are listed in order, without hidden, nested or other files"
else
    fail "project: the types files are listed in order (got '$listing')"
fi
listing="$("$CONSTRUCTO" check -v --dev --project="$PROJECTS/valid/patterns" 2>&1)"
if [[ "$listing" == *"/lib/b.types"* && "$listing" == *"/src/a.types"* && "$listing" != *"generated"* \
    && "$listing" != *"other.types"* ]]; then
    pass "project: include and exclude choose the files"
else
    fail "project: include and exclude choose the files (got '$listing')"
fi
listing="$("$CONSTRUCTO" check -v --dev --project="$PROJECTS/valid/imports-only" 2>&1)"
if [[ "$listing" == "OK: $PROJECTS/valid/imports-only (project example.importsonly)
OK: checked 0 file(s) in 1 project(s)" ]]; then
    pass "project: a project with no types files is accepted"
else
    fail "project: a project with no types files is accepted (got '$listing')"
fi

# A file that the project includes as a types file must not hold values.
err="$("$CONSTRUCTO" check --dev --project="$PROJECTS/rejected/values-in-types" 2>&1 >/dev/null)" && actual=0 || actual=$?
if [[ "$actual" -eq 1 && "$err" == "ERROR: The project includes this file as a types file but it holds data-values ($PROJECTS/rejected/values-in-types/notes.types, line 1, col 1)" ]]; then
    pass "project: a values file that the patterns include is reported"
else
    fail "project: a values file that the patterns include is reported (got exit $actual and '$err')"
fi

# Imports: every project that is imported is loaded once, before the project that imports it.
IMPORTS="$PROJECTS/valid-imports"
for root in diamond/app two-groups/app chain/a; do
    expect_exit "project/valid-imports/$root" 0 check --dev --project="$IMPORTS/$root"
done
project_names() { "$CONSTRUCTO" check -v --dev --project="$1" 2>&1 | sed -n 's|.*(project \([^,)]*\)[,)].*$|\1|p' | tr '\n' ' '; }
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
count="$("$CONSTRUCTO" check -v --dev --project="$IMPORTS/two-groups/app" 2>&1 | grep -c '(project ')"
if [[ "$count" -eq 2 ]]; then
    pass "imports: one project in several groups is still loaded once"
else
    fail "imports: one project in several groups is still loaded once (got $count)"
fi
# A relative source is relative to the folder of the project that imports, and not to the current folder.
if (cd "$IMPORTS/diamond" && "$CONSTRUCTO" check --dev --project=app >/dev/null 2>&1); then
    pass "imports: a relative source is relative to the importing project"
else
    fail "imports: a relative source is relative to the importing project"
fi
absolute="$(mktemp -d)"
printf '[project]\nname = "example.absolute"\n\n[[import]]\nproject-name = "example.lib"\npath = "%s"\n' \
    "$(cd "$IMPORTS/two-groups/lib" && pwd)" > "$absolute/constructo.toml"
expect_exit "imports: a source may be an absolute path" 0 check --dev --project="$absolute"
rm -f "$absolute/constructo.toml"
rmdir "$absolute"
# An error in an imported project is reported against that project.
broken="$(mktemp -d)"
mkdir "$broken/lib" "$broken/app"
printf '[project]\nname = "example.lib"\n' > "$broken/lib/constructo.toml"
echo "enum Lib = Lib(Missing);" > "$broken/lib/lib.types"
printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.lib"\npath = "../lib"\n' \
    > "$broken/app/constructo.toml"
echo "enum App = App;" > "$broken/app/app.types"
expect_exit "imports: a type error in an imported project fails the run" 1 check --dev --project="$broken/app"
listing="$("$CONSTRUCTO" check -v --dev --project="$broken/app" 2>/dev/null || true)"
if [[ "$listing" == *"FAILED: $broken/app/../lib (project example.lib, unverified)"* \
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
    err="$("$CONSTRUCTO" check --dev --project="$d/app" 2>&1 >/dev/null)" || actual=$?
    if [[ "$actual" -eq 2 && "$err" == "ERROR: $expected"* ]]; then
        pass "project/invalid-imports/$(basename "$d")"
    else
        fail "project/invalid-imports/$(basename "$d") (expected exit 2 with '$expected', got exit $actual and '$err')"
    fi
done
# Names: each project has its own names, which are written with a group or the full name of the project.
NAMES="$PROJECTS/valid-names"
expect_exit "project/valid-names/qualified" 0 check --dev --project="$NAMES/qualified/app" "$NAMES/qualified/app/values.values"
expect_exit "project/valid-names/keyword-parts" 0 check --dev --project="$NAMES/keyword-parts/app" "$NAMES/keyword-parts/app/values.values"
for case in bundle same-names group hide rename; do
    expect_exit "project/valid-names/$case" 0 check --dev --project="$NAMES/$case/app"
done
# A const of another project is a reference to a const, which needs the option as any other does.
listing="$("$CONSTRUCTO" check -v --dev --project="$PROJECTS/invalid-names/hide-unknown-name/app" 2>/dev/null || true)"
if [[ "$listing" == *"FAILED: "*"/app (project example.app)"* && "$listing" == *"OK: "*"/app/app.types (data-types)"* ]]; then
    pass "names: a bad hide fails its project in the verbose output, though its files pass"
else
    fail "names: a bad hide fails its project in the verbose output (got '$listing')"
fi
expect_exit "project/valid-names/not-transitive" 0 check --allow-refs --dev --project="$NAMES/not-transitive/app"
expect_exit "a const of another project is a reference" 1 check --dev --project="$NAMES/not-transitive/app"
# Each invalid case names its expected error, which starts the message, and the exit code is 1.
for d in "$PROJECTS"/invalid-names/*/; do
    d="${d%/}"
    expected="$(sed -n '1s|^// expect: ||p' "$d/app/app.types")"
    actual=0
    err="$("$CONSTRUCTO" check --dev --project="$d/app" 2>&1 >/dev/null)" || actual=$?
    if [[ "$actual" -eq 1 && "$err" == "ERROR: $expected"* ]]; then
        pass "project/invalid-names/$(basename "$d")"
    else
        fail "project/invalid-names/$(basename "$d") (expected exit 1 with '$expected', got exit $actual and '$err')"
    fi
done
# A name is shown as it would be written in the file the message is about.
listing="$("$CONSTRUCTO" check -v --dev --project="$NAMES/qualified/app" 2>/dev/null)"
if [[ "$listing" == *"const unit: example.lib::Shape"* && "$listing" == *"const holder: Holder"* ]]; then
    pass "names: a type of another project is shown in full, and one of the project itself is bare"
else
    fail "names: a type of another project is shown in full, and one of the project itself is bare (got '$listing')"
fi
listing="$("$CONSTRUCTO" check -v --dev --project="$NAMES/bundle/app" 2>/dev/null)"
if [[ "$listing" == *"const c: Shape"* && "$listing" != *"example.lib::"* ]]; then
    pass "names: a type of a project that is imported into local is bare"
else
    fail "names: a type of a project that is imported into local is bare (got '$listing')"
fi
# The files given with a project are written in the project's own namespace, with its own imports.
extra="$(mktemp)"
echo "enum Extra = Extra(lib::Shape);" > "$extra"
expect_exit "names: a types file that is given is written in the project" 0 check --dev --project="$NAMES/qualified/app" "$extra"
echo "enum Holder = Other;" > "$extra"
expect_exit "names: a type in a given file that the project declares is a duplicate" 1 \
    check --dev --project="$NAMES/qualified/app" "$extra"
echo "enum Extra = Extra(example.other::Shape);" > "$extra"
expect_exit "names: a given file cannot name a project that was not imported" 1 \
    check --dev --project="$NAMES/qualified/app" "$extra"
rm -f "$extra"
# The standard prelude is the built-in project constructo.prelude, which every project imports into local unless it
# imports it itself.
PRELUDES="$PROJECTS/valid-prelude"
for case in override explicit-local full-name no-group; do
    expect_exit "project/valid-prelude/$case" 0 check --dev --project="$PRELUDES/$case/app"
done
# An import of the prelude is the project's own statement, so --no-prelude leaves it alone.
for case in override explicit-local no-group; do
    expect_exit "prelude: --no-prelude keeps the explicit import in $case" 0 \
        check --no-prelude --dev --project="$PRELUDES/$case/app"
done
expect_exit "prelude: --no-prelude removes the implicit import" 1 \
    check --no-prelude --dev --project="$PRELUDES/full-name/app"
expect_exit "prelude: --prelude gives the implicit import back" 0 \
    check --config="$CONFIG/precedence/no-prelude.toml" --prelude --dev --project="$PRELUDES/full-name/app"
expect_exit "prelude: prelude = false in the settings file removes the implicit import" 1 \
    check --config="$CONFIG/precedence/no-prelude.toml" --dev --project="$PRELUDES/full-name/app"
# The imported projects get the prelude as well, so they can use Option and Bool.
nested="$(mktemp -d)"
mkdir "$nested/lib" "$nested/app"
printf '[project]\nname = "example.lib"\n' > "$nested/lib/constructo.toml"
echo "enum Lib = Lib(Option<Bool>);" > "$nested/lib/lib.types"
printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.lib"\npath = "../lib"\n' \
    > "$nested/app/constructo.toml"
echo "enum App = App;" > "$nested/app/app.types"
expect_exit "prelude: an imported project has the prelude too" 0 check --dev --project="$nested/app"
expect_exit "prelude: --no-prelude removes it from the imported project as well" 1 \
    check --no-prelude --dev --project="$nested/app"
rm -f "$nested/lib/constructo.toml" "$nested/lib/lib.types" "$nested/app/constructo.toml" "$nested/app/app.types"
rmdir "$nested/lib" "$nested/app" "$nested"
# The built-in project is never reported, and its file is shown by its URI.
listing="$("$CONSTRUCTO" check -v --dev --project="$PRELUDES/explicit-local/app" 2>/dev/null)"
if [[ "$listing" != *"constructo.prelude"* && "$listing" != *"constructo:/"* && "$listing" == *"(project example.app)"* ]]; then
    pass "prelude: the built-in project is not reported"
else
    fail "prelude: the built-in project is not reported (got '$listing')"
fi
expect_exit "prelude: the reserved prefix is refused in a project-file" 2 check --dev --project="$PROJECTS/invalid/reserved-name"
echo "enum Bool = No, Yes;" > "$forward_types"
expect_exit "prelude: files without a project may not declare a name that the prelude has" 1 check "$forward_types"
echo "enum Bool = No, Yes; const b: Bool = Yes;" > "$forward_types"
err="$("$CONSTRUCTO" check "$forward_types" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: Duplicate type Bool ("*"); first declared at constructo:/prelude/prelude.types, line 1, col 6" ]]; then
    pass "prelude: the prelude is in the scope of files without a project"
else
    fail "prelude: the prelude is in the scope of files without a project (got '$err')"
fi
expect_exit "prelude: --no-prelude is the way out for files without a project" 0 check --no-prelude "$forward_types"
# The values to check come after the project, and a failed project is reported as such.
expect_exit "project: values are type-checked against the project" 0 check --dev --project="$SHAPES" "$SHAPES/values/shapes.values"
echo "Circle { radius: \"wide\" }" > "$forward_values"
expect_exit "project: a type error in the values fails" 1 check --dev --project="$SHAPES" "$forward_values"
expect_exit "project: --stdin is checked against the project" 0 check --dev --project="$SHAPES" --stdin < "$SHAPES/values/shapes.values"
expect_exit "project: more types files may follow on the command line" 0 \
    check --dev --project="$SHAPES" "$EXAMPLES/valid/message.types" "$SHAPES/values/shapes.values"
expect_exit "project: but not after a values file" 1 \
    check --dev --project="$SHAPES" "$SHAPES/values/shapes.values" "$EXAMPLES/valid/message.types"
expect_exit "project: --no-type-check applies" 0 check --no-type-check --dev --project="$SHAPES" "$forward_values"
verbose="$("$CONSTRUCTO" check -v --dev --project="$SHAPES" "$forward_values" 2>/dev/null || true)"
verbose="${verbose%%$'\n'*}"
if [[ "$verbose" == "OK: $SHAPES (project com.example.shapes)" ]]; then
    pass "project: verbose mode reports the project"
else
    fail "project: verbose mode reports the project (got '$verbose')"
fi
# The prelude is in the scope of the project, so a project that declares Bool needs --no-prelude or an override.
bool_project="$(mktemp -d)"
printf '[project]\nname = "example.ownbool"\n' > "$bool_project/constructo.toml"
printf 'enum Bool = No, Yes;\nconst flag: Bool = Yes;\n' > "$bool_project/bool.types"
expect_exit "project: its own Bool is a duplicate of the prelude's" 1 check --dev --project="$bool_project"
err="$("$CONSTRUCTO" check --dev --project="$bool_project" 2>&1 >/dev/null || true)"
if [[ "$err" == "ERROR: Duplicate type Bool ("*"); first declared at constructo:/prelude/prelude.types, line 1, col 6" ]]; then
    pass "project: the duplicate is reported at the declaration"
else
    fail "project: the duplicate is reported at the declaration (got '$err')"
fi
expect_exit "project: its own Bool is fine with --no-prelude" 0 check --no-prelude --dev --project="$bool_project"
# Settings come from --config and the arguments only, never from the project-file.
expect_exit "project: --config still applies" 0 check --config="$CONFIG/precedence/no-prelude.toml" --dev --project="$bool_project"
expect_exit "project: --prelude overrides --config" 1 \
    check --config="$CONFIG/precedence/no-prelude.toml" --prelude --dev --project="$bool_project"
verbose="$("$CONSTRUCTO" check -v --dev --project="$bool_project" 2>/dev/null || true)"
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
    && [[ "$(printed | grep '^# ' | grep -vc '^# cache = ')" -eq 11 ]] \
    && printed | grep -qxF '# cache = "FOLDER"'; then
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

# The lock file. The digests are worked out here with sha256sum, so they do not depend on the code that is being tested.
echo "Lock file"
sha() { if command -v sha256sum >/dev/null; then sha256sum; else shasum -a 256; fi | cut -d' ' -f1; }
digest_of() { tr -d '\r' < "$1" | sha; }
lk="$(mktemp -d)"
mkdir "$lk/lib" "$lk/app"
printf '[project]\nname = "example.lib"\n' > "$lk/lib/constructo.toml"
printf 'enum Shape = Circle;\r\n' > "$lk/lib/shapes.types"
printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.lib"\npath = "../lib"\n' > "$lk/app/constructo.toml"
printf 'enum App = App;\n' > "$lk/app/app.types"
# Writes the lock file of the app from the files of the lib as they are now, with the extra lines given as the files.
write_lock() {
    local toml shapes project
    toml="$(digest_of "$lk/lib/constructo.toml")"
    shapes="$(digest_of "$lk/lib/shapes.types")"
    project="$(printf 'constructo.toml sha256:%s\nshapes.types sha256:%s\n' "$toml" "$shapes" | sha)"
    {
        printf 'version = 1\n\n[[project]]\nname = "example.lib"\ndigest = "sha256:%s"\n\n[project.files]\n' "$project"
        printf '"constructo.toml" = "sha256:%s"\n"shapes.types" = "sha256:%s"\n' "$toml" "$shapes"
    } > "$lk/app/constructo.lock"
}
# The first line of the errors, or the exit code, of a run on the app.
lock_errors() { "$CONSTRUCTO" check "$@" --project="$lk/app" 2>&1 >/dev/null || true; }
lock_exit() { local code=0; "$CONSTRUCTO" check "$@" --project="$lk/app" >/dev/null 2>&1 || code=$?; echo "$code"; }
lock_expect() {
    local name="$1" want="$2" pattern="$3"; shift 3
    local code out
    code="$(lock_exit "$@")"
    out="$(lock_errors "$@")"
    if [[ "$code" -eq "$want" && "$out" == $pattern ]]; then pass "$name"; else fail "$name (exit $code, got '$out')"; fi
}
lock_expect "lock: a project that imports another needs a lock file" 1 \
    "ERROR: The project imports example.lib, but there is no constructo.lock; run \`constructo project update\` to make one ($lk/app/constructo.toml, line 5, col 16)"
lock_expect "lock: --dev does not need the lock file" 0 ""  --dev
write_lock
lock_expect "lock: a lock that matches the files is accepted" 0 ""
printf 'enum Shape = Square;\r\n' > "$lk/lib/shapes.types"
lock_expect "lock: a changed file is a mismatch" 1 \
    "ERROR: Checksum mismatch for shapes.types of project example.lib: the lock has sha256:* but the file has sha256:* ($lk/app/constructo.lock, line 9, col 18)"
lock_expect "lock: --dev skips the verification" 0 "" --dev
lock_expect "lock: --no-dev undoes --dev" 1 "ERROR: Checksum mismatch for shapes.types*" --dev --no-dev
printf 'enum Shape = Circle;\r\n' > "$lk/lib/shapes.types"
lock_expect "lock: the digest is of the file with the line endings of LF" 0 ""
printf 'enum Shape = Circle;\n' > "$lk/lib/shapes.types"
lock_expect "lock: LF and CRLF line endings give the same digest" 0 ""
printf '\xEF\xBB\xBFenum Shape = Circle;\n' > "$lk/lib/shapes.types"
lock_expect "lock: a byte order mark does not change the digest" 0 ""
printf 'enum Shape = Circle;\r\n' > "$lk/lib/shapes.types"
printf 'enum More = M;\n' > "$lk/lib/more.types"
lock_expect "lock: a file that the lock does not have is reported" 1 \
    "ERROR: The file more.types of project example.lib is not in constructo.lock; run \`constructo project update\` to record it ($lk/app/constructo.lock, line 4, col 8)"
rm -f "$lk/lib/more.types"
mv "$lk/lib/shapes.types" "$lk/shapes.types.moved"
lock_expect "lock: a file that has gone is reported" 1 \
    "ERROR: The lock has the file shapes.types of project example.lib, which is no longer in the project ($lk/app/constructo.lock, line 9, col 18)"
mv "$lk/shapes.types.moved" "$lk/lib/shapes.types"
printf '[project]\nname = "example.lib"\n# changed\n' > "$lk/lib/constructo.toml"
lock_expect "lock: a change to the project-file of the import is a mismatch" 1 \
    "ERROR: Checksum mismatch for constructo.toml of project example.lib*"
printf '[project]\nname = "example.lib"\n' > "$lk/lib/constructo.toml"
lock_expect "lock: the files are accepted again once they are as they were" 0 ""
# A lock that lists a project that is not imported, and one that is damaged.
cp "$lk/app/constructo.lock" "$lk/good.lock"
printf '\n[[project]]\nname = "example.old"\ndigest = "sha256:%s"\n\n[project.files]\n' "$(printf '' | sha)" >> "$lk/app/constructo.lock"
lock_expect "lock: a project in the lock that is not imported is reported" 1 \
    "ERROR: The lock has the project example.old, which is not imported ($lk/app/constructo.lock, line 12, col 8)"
printf 'version = 2\n' > "$lk/app/constructo.lock"
lock_expect "lock: a lock of another version is exit 2" 2 \
    "ERROR: Unsupported lock file version 2; this version of constructo reads version 1 ($lk/app/constructo.lock, line 1, col 11)"
printf 'not toml [' > "$lk/app/constructo.lock"
lock_expect "lock: a lock that does not parse is exit 2" 2 "ERROR: * ($lk/app/constructo.lock, line 1, col *)"
lock_expect "lock: --dev skips a lock that does not parse" 0 "" --dev
cp "$lk/good.lock" "$lk/app/constructo.lock"
# The check of types and values comes after the lock, so a mismatch is reported without them.
printf 'Nonsense(\n' > "$lk/values.values"
printf 'enum Shape = Square;\n' > "$lk/lib/shapes.types"
code=0
"$CONSTRUCTO" check --project="$lk/app" "$lk/values.values" >/dev/null 2>&1 || code=$?
if [[ "$code" -eq 1 ]] && [[ "$("$CONSTRUCTO" check --project="$lk/app" "$lk/values.values" 2>&1)" == "ERROR: Checksum mismatch"* ]]; then
    pass "lock: the lock is verified before the values are read"
else
    fail "lock: the lock is verified before the values are read"
fi
# The setting can come from a configuration file, and an option overrides it.
printf '[check]\ndev = true\n' > "$lk/settings.toml"
lock_expect "lock: dev can be set in the configuration file" 0 "" --config="$lk/settings.toml"
lock_expect "lock: --no-dev overrides the configuration file" 1 "ERROR: Checksum mismatch*" \
    --config="$lk/settings.toml" --no-dev
if prints "dev = false" && prints "dev = true" --dev && prints "dev = true" --config="$lk/settings.toml" \
    && prints "dev = false" --config="$lk/settings.toml" --no-dev && prints "dev = false" --dev --no-dev; then
    pass "lock: print-config shows dev"
else
    fail "lock: print-config shows dev"
fi
# Verbose mode says which projects were verified, and counts the projects.
write_lock
loud="$("$CONSTRUCTO" check -v --project="$lk/app" 2>&1)"
expected_loud="OK: $lk/app/../lib (project example.lib, verified)
OK: $lk/app/../lib/shapes.types (data-types)
OK: $lk/app (project example.app)
OK: $lk/app/app.types (data-types)
OK: checked 2 file(s) in 2 project(s)"
if [[ "$loud" == "$expected_loud" ]]; then
    pass "lock: verbose mode says that an imported project is verified, and the root says nothing"
else
    fail "lock: verbose mode says verified (got '$loud')"
fi
loud="$("$CONSTRUCTO" check -v --dev --project="$lk/app" 2>&1)"
if [[ "$loud" == *"(project example.lib, unverified)"* && "$loud" == *"(project example.app)"* \
    && "$loud" == *"OK: checked 2 file(s) in 2 project(s)" ]]; then
    pass "lock: verbose mode says that a project is unverified under --dev"
else
    fail "lock: verbose mode says unverified under --dev (got '$loud')"
fi
# A run with no project, and a project with nothing to record, have no lock to verify.
expect_exit "lock: files alone have no lock" 0 check "$TYPED/schema.types"
expect_exit "lock: a project with no imports needs no lock file" 0 check --project="$SHAPES"
expect_exit "lock: a project that imports only the prelude needs no lock file" 0 check --project="$PRELUDES/explicit-local/app"
rm -f "$lk/lib/constructo.toml" "$lk/lib/shapes.types" "$lk/app/constructo.toml" "$lk/app/app.types"
rm -f "$lk/app/constructo.lock" "$lk/good.lock" "$lk/values.values" "$lk/settings.toml"
rmdir "$lk/lib" "$lk/app" "$lk"
echo ""

# Updating the lock. The lock file that is written is compared with one made here from sha256sum.
echo "Updating the lock"
up="$(mktemp -d)"
mkdir "$up/lib" "$up/app"
printf '[project]\nname = "example.lib"\n' > "$up/lib/constructo.toml"
printf 'enum Shape = Circle;\r\n' > "$up/lib/shapes.types"
printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.lib"\npath = "../lib"\n' > "$up/app/constructo.toml"
# The lock that the update is expected to write, from the files of the lib as they are now.
expected_lock() {
    local toml shapes project
    toml="$(digest_of "$up/lib/constructo.toml")"
    shapes="$(digest_of "$up/lib/shapes.types")"
    project="$(printf 'constructo.toml sha256:%s\nshapes.types sha256:%s\n' "$toml" "$shapes" | sha)"
    printf '# This file is generated by `constructo project update`. Do not edit it.\n'
    printf 'version = 1\n\n[[project]]\nname = "example.lib"\ndigest = "sha256:%s"\n\n[project.files]\n' "$project"
    printf '"constructo.toml" = "sha256:%s"\n"shapes.types" = "sha256:%s"\n' "$toml" "$shapes"
}
out="$("$CONSTRUCTO" project update "$up/app" 2>&1)" || true
if [[ "$out" == "Added the project example.lib (sha256:"*")" && "$(wc -l <<< "$out")" -eq 1 ]]; then
    pass "update: a new lock says that it added the project"
else
    fail "update: a new lock says that it added the project (got '$out')"
fi
if [[ "$(cat "$up/app/constructo.lock")" == "$(expected_lock)" ]]; then
    pass "update: the lock has the digests that sha256sum gives"
else
    fail "update: the lock has the digests that sha256sum gives"
fi
expect_exit "update: check accepts the lock that was written" 0 check --project="$up/app"
out="$("$CONSTRUCTO" project update "$up/app" 2>&1)" || true
if [[ -z "$out" ]]; then pass "update: a lock that is up to date prints nothing"; else fail "update: up to date prints nothing (got '$out')"; fi
out="$(cd "$up/app" && "$CONSTRUCTO" project update 2>&1)" || true
if [[ -z "$out" ]]; then pass "update: the folder defaults to the current folder"; else fail "update: default folder (got '$out')"; fi
printf 'enum Shape = Square;\n' > "$up/lib/shapes.types"
expect_exit "update: check rejects a changed file" 1 check --project="$up/app"
out="$("$CONSTRUCTO" project update "$up/app" 2>&1)" || true
if [[ "$out" == "Changed the file shapes.types of project example.lib (sha256:"*" to sha256:"*")" ]]; then
    pass "update: a changed file is reported with the old and the new digest"
else
    fail "update: a changed file is reported (got '$out')"
fi
expect_exit "update: check accepts the changed file once it is recorded" 0 check --project="$up/app"
printf 'not a lock [' > "$up/app/constructo.lock"
out="$("$CONSTRUCTO" project update "$up/app" 2>&1)" || true
if [[ "$out" == "Replaced the constructo.lock that could not be used: "* ]]; then
    pass "update: a lock that does not parse is replaced and the first line says so"
else
    fail "update: a lock that does not parse is replaced (got '$out')"
fi
expect_exit "update: check accepts the replaced lock" 0 check --project="$up/app"
# Nothing is type-checked.
printf 'enum Bad = Missing(Nothing);\n' > "$up/lib/bad.types"
expect_exit "update: a type error does not stop an update" 0 project update "$up/app"
rm -f "$up/lib/bad.types"
# A project that imports nothing has nothing to record, so its old lock is removed.
printf '[project]\nname = "example.app"\n' > "$up/app/constructo.toml"
out="$("$CONSTRUCTO" project update "$up/app" 2>&1)" || true
if [[ "$out" == "Removed the project example.lib (sha256:"*")" && ! -e "$up/app/constructo.lock" ]]; then
    pass "update: a project with nothing to record has no lock"
else
    fail "update: a project with nothing to record has no lock (got '$out')"
fi
expect_exit "update: no lock is made for a project with nothing to record" 0 project update "$up/app"
if [[ ! -e "$up/app/constructo.lock" ]]; then pass "update: and none was written"; else fail "update: and none was written"; fi
# Problems are exit 2, as for check.
expect_exit "update: a folder that does not exist is exit 2" 2 project update "$up/nowhere"
printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.other"\npath = "../lib"\n' > "$up/app/constructo.toml"
expect_exit "update: a project that cannot be loaded is exit 2" 2 project update "$up/app"
rm -f "$up/lib/constructo.toml" "$up/lib/shapes.types" "$up/app/constructo.toml" "$up/app/constructo.lock"
rmdir "$up/lib" "$up/app" "$up"
echo ""

# Projects in a git repository (Step 6). Only `project update` fetches, and it records the commit in the lock.
echo "=== Projects in a repository ==="
if command -v git >/dev/null 2>&1; then
    rp="$(mktemp -d)"
    mkdir "$rp/repo" "$rp/app"
    printf '[project]\nname = "example.lib"\n' > "$rp/repo/constructo.toml"
    printf 'enum Shape = Circle;\n' > "$rp/repo/shapes.types"
    # The git that builds the fixtures gets an environment of its own, with no configuration of the system or the
    # user: a variable such as GIT_DIR, which git sets when it runs a hook, would point it at another repository, and
    # a hook or a signing setting of the developer would run on the commits.
    git_isolated() { env -i PATH="$PATH" GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null git "$@"; }
    git_as_test() { git_isolated -c user.name=Test -c user.email=test@example.com -c commit.gpgsign=false "$@"; }
    git_in_repo() { git_as_test -C "$rp/repo" "$@"; }
    git_in_repo init --quiet --initial-branch=main
    git_in_repo add -A
    git_in_repo commit --quiet -m first
    commit="$(git_in_repo rev-parse HEAD)"
    printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.lib"\nrepo = "%s"\n' "$rp/repo" \
        > "$rp/app/constructo.toml"
    export CONSTRUCTO_CACHE="$rp/cache"
    out="$("$CONSTRUCTO" project update "$rp/app" 2>&1)" || true
    if [[ "$out" == "Added the project example.lib from $rp/repo at $commit (sha256:"*")" ]]; then
        pass "repo: update fetches the project and says what it added"
    else
        fail "repo: update fetches the project and says what it added (got '$out')"
    fi
    if grep -q "^commit = \"$commit\"\$" "$rp/app/constructo.lock" && grep -q '^repo = ' "$rp/app/constructo.lock"; then
        pass "repo: the lock records the repository and the commit"
    else
        fail "repo: the lock records the repository and the commit"
    fi
    out="$("$CONSTRUCTO" project update "$rp/app" 2>&1)" || true
    if [[ -z "$out" ]]; then
        pass "repo: a second update says nothing"
    else
        fail "repo: a second update says nothing (got '$out')"
    fi
    # Git is given only the variables that constructo names, so ones that would point it at another repository or at
    # other programs do not reach it.
    out="$(GIT_DIR=/nonexistent GIT_WORK_TREE=/nonexistent GIT_EXEC_PATH=/nonexistent GIT_CONFIG_GLOBAL=/nonexistent \
        "$CONSTRUCTO" project update "$rp/app" 2>&1)" && status=0 || status=$?
    if [[ $status -eq 0 && -z "$out" ]]; then
        pass "repo: update ignores GIT_ variables in the environment"
    else
        fail "repo: update ignores GIT_ variables in the environment (exit $status, got '$out')"
    fi
    # ssh is run in batch mode unless the user says how to run it. A stand-in for ssh on the PATH writes down its
    # arguments and runs the command that git gives it here, which is what ssh would run on the host.
    mkdir "$rp/bin" "$rp/sshapp" "$rp/sshhome"
    cat > "$rp/bin/ssh" <<STAND_IN
#!/bin/sh
printf '%s\n' "\$*" >> "$rp/ssh-arguments"
for last in "\$@"; do :; done
exec sh -c "\$last"
STAND_IN
    chmod +x "$rp/bin/ssh"
    printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.lib"\nrepo = "localhost:%s"\n' \
        "$rp/repo" > "$rp/sshapp/constructo.toml"
    printf '[core]\n\tsshCommand = ssh -o ConfigMarker=yes\n' > "$rp/sshhome/.gitconfig"
    ssh_update() {
        rm -f "$rp/ssh-arguments"
        env PATH="$rp/bin:$PATH" "$@" "$CONSTRUCTO" project update "$rp/sshapp" > /dev/null 2>&1 \
            && status=0 || status=$?
        said="$(cat "$rp/ssh-arguments" 2>/dev/null || true)"
    }
    # Both places of the user's git configuration are empty folders, so that no core.sshCommand is found there. A
    # core.sshCommand in the system configuration would still be found, since constructo reads that for the clone.
    ssh_update env -u GIT_SSH_COMMAND -u GIT_SSH HOME="$rp/nohome" XDG_CONFIG_HOME="$rp/nohome"
    if [[ $status -eq 0 && "$said" == *"BatchMode=yes"* ]]; then
        pass "repo: ssh is run in batch mode"
    else
        fail "repo: ssh is run in batch mode (exit $status, ssh was given '$said')"
    fi
    ssh_update env GIT_SSH_COMMAND="ssh -o EnvironmentMarker=yes"
    if [[ $status -eq 0 && "$said" == *"EnvironmentMarker=yes"* && "$said" != *"BatchMode"* ]]; then
        pass "repo: GIT_SSH_COMMAND of the user is used as it is"
    else
        fail "repo: GIT_SSH_COMMAND of the user is used as it is (exit $status, ssh was given '$said')"
    fi
    ssh_update env -u GIT_SSH_COMMAND -u GIT_SSH HOME="$rp/sshhome" XDG_CONFIG_HOME="$rp/sshhome"
    if [[ $status -eq 0 && "$said" == *"ConfigMarker=yes"* && "$said" != *"BatchMode"* ]]; then
        pass "repo: core.sshCommand of the user is used as it is"
    else
        fail "repo: core.sshCommand of the user is used as it is (exit $status, ssh was given '$said')"
    fi
    rm -f "$rp/ssh-arguments" "$rp/bin/ssh" "$rp/sshhome/.gitconfig" "$rp/sshapp/constructo.toml" \
        "$rp/sshapp/constructo.lock"
    rmdir "$rp/bin" "$rp/sshapp" "$rp/sshhome"
    printf 'enum More = M;\n' > "$rp/repo/more.types"
    git_in_repo add -A
    git_in_repo commit --quiet -m second
    moved="$(git_in_repo rev-parse HEAD)"
    out="$("$CONSTRUCTO" project update "$rp/app" 2>&1)" || true
    if [[ "$out" == "Moved the project example.lib from commit $commit to $moved"* ]]; then
        pass "repo: update says when the commit moves"
    else
        fail "repo: update says when the commit moves (got '$out')"
    fi
    # The lock now holds the commit, so `check` uses the cache and never the network, even after the repository
    # moves on.
    printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.lib"\nrepo = "%s"\n' "$rp/repo" \
        > "$rp/app/constructo.toml"
    printf 'enum Pick = A(example.lib::Shape);\n' > "$rp/app/pick.types"
    expect_exit "repo: check passes from the cache" 0 check --project="$rp/app"
    # The cache can be given explicitly, by --cache or by the configuration file, and that wins over the environment.
    CONSTRUCTO_CACHE="$rp/empty" expect_exit "repo: --cache wins over CONSTRUCTO_CACHE" 0 \
        check --cache="$rp/cache" --project="$rp/app"
    mkdir "$rp/conf"
    printf '[global]\ncache = "../cache"\n' > "$rp/conf/settings.toml"
    out="$(cd / && CONSTRUCTO_CACHE="$rp/empty" "$CONSTRUCTO" --config="$rp/conf/settings.toml" check \
        --project="$rp/app" 2>&1)" && status=0 || status=$?
    if [[ $status -eq 0 ]]; then
        pass "repo: a relative cache in a configuration file is relative to the file"
    else
        fail "repo: a relative cache in a configuration file is relative to the file (exit $status, got '$out')"
    fi
    out="$(cd "$rp" && "$CONSTRUCTO" check --print-config --cache=cache 2>&1)" || true
    if grep -qxF "cache = \"$rp/cache\"" <<< "$out"; then
        pass "repo: --print-config writes the cache that was given as an absolute path"
    else
        fail "repo: --print-config writes the cache that was given as an absolute path (got '$out')"
    fi
    out="$(CONSTRUCTO_CACHE=cache "$CONSTRUCTO" check --project="$rp/app" 2>&1)" && status=0 || status=$?
    if [[ $status -eq 2 && "$out" == *"CONSTRUCTO_CACHE is cache, which is not an absolute path"* ]]; then
        pass "repo: a relative CONSTRUCTO_CACHE is refused"
    else
        fail "repo: a relative CONSTRUCTO_CACHE is refused (exit $status, got '$out')"
    fi
    out="$("$CONSTRUCTO" check --verbose --project="$rp/app" 2>&1)" || true
    if [[ "$out" == *"(project example.lib, verified)"* ]]; then
        pass "repo: verbose says the project from a repository is verified"
    else
        fail "repo: verbose says the project from a repository is verified (got '$out')"
    fi
    expect_exit "repo: check passes under --dev too" 0 check --dev --project="$rp/app"
    out="$("$CONSTRUCTO" check --dev --verbose --project="$rp/app" 2>&1)" || true
    if [[ "$out" == *"(project example.lib, verified)"* ]]; then
        pass "repo: --dev does not make a project from a repository unverified"
    else
        fail "repo: --dev does not make a project from a repository unverified (got '$out')"
    fi
    git_in_repo commit --quiet --allow-empty -m third
    expect_exit "repo: check does not follow a repository that moved on" 0 check --project="$rp/app"
    # An empty cache has not got the locked commit, and check does not fetch it.
    mkdir "$rp/empty"
    out="$(CONSTRUCTO_CACHE="$rp/empty" "$CONSTRUCTO" check --project="$rp/app" 2>&1)" && status=0 || status=$?
    if [[ $status -eq 1 \
        && "$out" == *"is not in the cache; run \`constructo project update\` to fetch it ("*"line 6, col 8)" ]]; then
        pass "repo: check says to run update when the cache has not got the commit"
    else
        fail "repo: check says to run update when the cache has not got the commit (exit $status, got '$out')"
    fi
    if [[ ! -e "$rp/empty/git" ]]; then pass "repo: and it did not fetch"; else fail "repo: and it did not fetch"; fi
    rmdir "$rp/empty"
    # A file in the cache that was changed is a checksum mismatch, whatever --dev says.
    locked="$(sed -n 's/^commit = "\(.*\)"$/\1/p' "$rp/app/constructo.lock")"
    cached="$(echo "$CONSTRUCTO_CACHE"/git/*/"$locked")"
    chmod u+w "$cached/shapes.types"
    printf 'enum Shape = Square;\n' > "$cached/shapes.types"
    out="$("$CONSTRUCTO" check --dev --project="$rp/app" 2>&1)" && status=0 || status=$?
    if [[ $status -eq 1 && "$out" == "ERROR: Checksum mismatch for shapes.types of project example.lib"* ]]; then
        pass "repo: a changed file in the cache is a checksum mismatch, even with --dev"
    else
        fail "repo: a changed file in the cache is a checksum mismatch, even with --dev (exit $status, got '$out')"
    fi
    # A link in the cache that leads out of the clone is refused, and nothing of what it leads to is shown.
    printf 'SECRET-CONTENT\n' > "$rp/secret"
    rm -f "$cached/shapes.types"
    ln -s "$rp/secret" "$cached/shapes.types"
    out="$("$CONSTRUCTO" check --dev --project="$rp/app" 2>&1)" && status=0 || status=$?
    if [[ $status -eq 2 \
        && "$out" == *"shapes.types of the project example.lib leads to $rp/secret, which is outside the repository"* \
        && "$out" != *"SECRET-CONTENT"* ]]; then
        pass "repo: check refuses a link that leaves the clone"
    else
        fail "repo: check refuses a link that leaves the clone (exit $status, got '$out')"
    fi
    rm -f "$cached/shapes.types"
    printf 'enum Shape = Circle;\n' > "$cached/shapes.types"
    # With no lock there is nothing that says which commit to use.
    mv "$rp/app/constructo.lock" "$rp/app/constructo.lock.off"
    out="$("$CONSTRUCTO" check --dev --project="$rp/app" 2>&1)" && status=0 || status=$?
    if [[ $status -eq 1 \
        && "$out" == *"but there is no constructo.lock; run \`constructo project update\` to make one"* ]]; then
        pass "repo: no lock is an error for a project from a repository, even with --dev"
    else
        fail "repo: no lock is an error for a project from a repository, even with --dev (exit $status, got '$out')"
    fi
    # A lock that does not parse is exit 2, and --dev does not excuse it for a project from a repository.
    printf 'not a lock' > "$rp/app/constructo.lock"
    expect_exit "repo: a lock that does not parse is exit 2" 2 check --project="$rp/app"
    expect_exit "repo: and is exit 2 under --dev when a repository needs it" 2 check --dev --project="$rp/app"
    rm -f "$rp/app/constructo.lock"
    mv "$rp/app/constructo.lock.off" "$rp/app/constructo.lock"
    rm -f "$rp/app/pick.types"
    printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.lib"\nrepo = "%s"\n%s\n' \
        "$rp/repo" 'rev = "nothing-here"' > "$rp/app/constructo.toml"
    expect_exit "repo: a rev that does not exist is exit 2" 2 project update "$rp/app"
    printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.lib"\nrepo = "ext::sh -c ls"\n' \
        > "$rp/app/constructo.toml"
    expect_exit "repo: a remote helper is refused with exit 2" 2 project update "$rp/app"
    # A link that leaves the clone stops an update, and so does a path import that leaves it.
    printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.lib"\nrepo = "%s"\n' "$rp/repo" \
        > "$rp/app/constructo.toml"
    ln -s "$rp/secret" "$rp/repo/bad.types"
    git_in_repo add -A
    git_in_repo commit --quiet -m link
    out="$("$CONSTRUCTO" project update "$rp/app" 2>&1)" && status=0 || status=$?
    if [[ $status -eq 2 && "$out" == *"bad.types of the project example.lib leads to $rp/secret"* ]]; then
        pass "repo: update refuses a link that leaves the clone"
    else
        fail "repo: update refuses a link that leaves the clone (exit $status, got '$out')"
    fi
    rm -f "$rp/repo/bad.types"
    printf '[project]\nname = "example.lib"\n\n[[import]]\nproject-name = "example.other"\npath = "../elsewhere"\n' \
        > "$rp/repo/constructo.toml"
    git_in_repo add -A
    git_in_repo commit --quiet -m escape
    out="$("$CONSTRUCTO" project update "$rp/app" 2>&1)" && status=0 || status=$?
    if [[ $status -eq 2 && "$out" == *"leaves the repository that the project example.lib is in"* ]]; then
        pass "repo: update refuses a path that leaves the clone"
    else
        fail "repo: update refuses a path that leaves the clone (exit $status, got '$out')"
    fi
    # A link in a repository is checked out as a link, whatever the configuration of the user says, so that the digest
    # is the same on every machine. A configuration with `symlinks = false` would give a plain file with the target.
    mkdir "$rp/linkrepo" "$rp/linkapp" "$rp/home"
    printf '[core]\n\tsymlinks = false\n' > "$rp/home/.gitconfig"
    printf '[project]\nname = "example.lnk"\n' > "$rp/linkrepo/constructo.toml"
    printf 'enum Real = R;\n' > "$rp/linkrepo/real.types"
    ln -s real.types "$rp/linkrepo/alias.types"
    git_in_link() { git_as_test -C "$rp/linkrepo" "$@"; }
    git_in_link init --quiet --initial-branch=main
    git_in_link add -A
    git_in_link commit --quiet -m links
    printf '[project]\nname = "example.app"\n\n[[import]]\nproject-name = "example.lnk"\nrepo = "%s"\n' "$rp/linkrepo" \
        > "$rp/linkapp/constructo.toml"
    out="$(HOME="$rp/home" XDG_CONFIG_HOME="$rp/home" "$CONSTRUCTO" project update "$rp/linkapp" 2>&1)" \
        && status=0 || status=$?
    linked="$(sed -n 's/^commit = "\(.*\)"$/\1/p' "$rp/linkapp/constructo.lock")"
    if [[ $status -eq 0 && -L "$(echo "$CONSTRUCTO_CACHE"/git/*/"$linked")/alias.types" ]]; then
        pass "repo: a link is checked out as a link whatever the git configuration says"
    else
        fail "repo: a link is checked out as a link whatever the git configuration says (exit $status, got '$out')"
    fi
    # An update does not reuse a checkout in the cache that is not what a fresh one is, since it would record the
    # changed file as a part of the commit.
    printf 'enum Changed = C;\n' > "$(echo "$CONSTRUCTO_CACHE"/git/*/"$linked")/real.types"
    out="$("$CONSTRUCTO" project update "$rp/linkapp" 2>&1)" && status=0 || status=$?
    if [[ $status -eq 2 && "$out" == *"in the cache has been changed: real.types is not what it was"* ]]; then
        pass "repo: update refuses a checkout in the cache that was changed"
    else
        fail "repo: update refuses a checkout in the cache that was changed (exit $status, got '$out')"
    fi
    # The checkout does not use the configuration of the user: a filter that it names, which a .gitattributes in the
    # repository can ask for, is not run.
    printf '[filter "evil"]\n\tsmudge = sh -c '"'"'touch %s/ran && cat'"'"'\n' "$rp" > "$rp/home/.gitconfig"
    printf '* filter=evil\n' > "$rp/linkrepo/.gitattributes"
    git_in_link add -A
    git_in_link commit --quiet -m attributes
    out="$(HOME="$rp/home" XDG_CONFIG_HOME="$rp/home" "$CONSTRUCTO" project update "$rp/linkapp" 2>&1)" \
        && status=0 || status=$?
    if [[ $status -eq 0 && ! -e "$rp/ran" ]]; then
        pass "repo: update does not run a filter from the git configuration of the user"
    else
        fail "repo: update does not run a filter from the git configuration of the user (exit $status, got '$out')"
    fi
    rm -f "$rp/ran" "$rp/linkrepo/.gitattributes"
    unset CONSTRUCTO_CACHE
    rm -f "$rp/secret"
    rm -rf "$rp/repo/.git" "$rp/cache/git"
    rm -f "$rp/repo/constructo.toml" "$rp/repo/shapes.types" "$rp/repo/more.types" "$rp/app/constructo.toml" \
        "$rp/app/constructo.lock"
    rm -rf "$rp/linkrepo/.git" "$rp/cache/git"
    rm -f "$rp/linkrepo/constructo.toml" "$rp/linkrepo/real.types" "$rp/linkrepo/alias.types" \
        "$rp/linkapp/constructo.toml" "$rp/linkapp/constructo.lock" "$rp/home/.gitconfig"
    rm -f "$rp/conf/settings.toml"
    rmdir "$rp/linkrepo" "$rp/linkapp" "$rp/home" "$rp/conf"
    rmdir "$rp/repo" "$rp/app" "$rp/cache" "$rp"
else
    echo "  SKIP: git is not installed"
fi
echo ""

# The version is printed by `constructo --version` and `constructo version`, and by nothing else.
version_flag="$("$CONSTRUCTO" --version 2>&1)" || true
version_command="$("$CONSTRUCTO" version 2>&1)" || true
if [[ "$version_flag" == "constructo "[0-9]* && "$version_command" == "$version_flag" ]]; then
    pass "version: the subcommand and the top-level option say the same"
else
    fail "version: the subcommand and the top-level option say the same (got '$version_flag' and '$version_command')"
fi
expect_exit "version: the subcommand is exit 0 even with a bad --config" 0 --config /nonexistent/settings.toml version
expect_exit "version: check has no --version" 2 check --version
expect_exit "version: project has no --version" 2 project --version
# `project init` writes a project-file with the defaults, and never changes one that is there.
init_dir="$(mktemp -d)"
mkdir "$init_dir/My Shapes"
out="$("$CONSTRUCTO" project init "$init_dir/My Shapes" 2>&1)" || true
if [[ "$out" == "Wrote "*"constructo.toml for the project example.my_shapes;"* ]]; then
    pass "init: a new project-file says what it wrote"
else
    fail "init: a new project-file says what it wrote (got '$out')"
fi
expect_exit "init: check accepts the project-file that was written" 0 check --project="$init_dir/My Shapes"
printf '# mine\n' >> "$init_dir/My Shapes/constructo.toml"
out="$("$CONSTRUCTO" project init "$init_dir/My Shapes" 2>&1)" || true
if [[ "$out" == *"exists already, so it was not changed" && "$(tail -n 1 "$init_dir/My Shapes/constructo.toml")" == "# mine" ]]; then
    pass "init: a project-file that exists is left as it is"
else
    fail "init: a project-file that exists is left as it is (got '$out')"
fi
expect_exit "init: a folder that is not there is exit 2" 2 project init "$init_dir/missing"
expect_exit "init: project with no subcommand is exit 2" 2 project
mkdir "$init_dir/here"
out="$(cd "$init_dir/here" && "$CONSTRUCTO" project init 2>&1)" || true
if [[ -f "$init_dir/here/constructo.toml" ]]; then pass "init: the folder defaults to the current folder"; else fail "init: default folder (got '$out')"; fi
rm -f "$init_dir/My Shapes/constructo.toml" "$init_dir/here/constructo.toml"
rmdir "$init_dir/My Shapes" "$init_dir/here" "$init_dir"
echo ""

# `convert --to=json-exact` writes the values in an explicit form, and the output is compared with files as text.
CONVERT="$(dirname "$0")/../syntax/examples/convert"
conv_dir="$(mktemp -d)"

# Runs convert, and compares the standard output with the expected file and the exit code with 0.
expect_json() {
    local name="$1" expected="$2"; shift 2
    local actual=0
    "$CONSTRUCTO" convert "$@" >"$conv_dir/out" 2>"$conv_dir/err" || actual=$?
    if [[ "$actual" -eq 0 ]] && cmp -s "$conv_dir/out" "$expected" && [[ ! -s "$conv_dir/err" ]]; then
        pass "convert: $name"
    else
        fail "convert: $name (exit $actual, output differs: $(cmp "$conv_dir/out" "$expected" 2>&1 | head -n 1))"
    fi
}

# Runs convert and requires the exit code, an empty standard output and a first line of the errors that starts as given,
# where the message is a pattern, so a `*` in it stands for anything.
expect_convert_error() {
    local name="$1" code="$2" message="$3"; shift 3
    local actual=0
    "$CONSTRUCTO" convert "$@" >"$conv_dir/out" 2>"$conv_dir/err" || actual=$?
    local first
    first="$(head -n 1 "$conv_dir/err")"
    if [[ "$actual" -eq "$code" && ! -s "$conv_dir/out" && "$first" == $message* ]]; then
        pass "convert: $name"
    else
        fail "convert: $name (exit $actual, stdout '$(cat "$conv_dir/out")', first error '$first')"
    fi
}

expect_json "every kind of value, pretty" "$CONVERT/kinds.json" --to=json-exact --to-array "$CONVERT/kinds.values"
expect_json "every kind of value, compact" "$CONVERT/kinds.compact.json" --to=json-exact --to-array --compact \
    "$CONVERT/kinds.values"
expect_json "a nested value, pretty" "$CONVERT/nested.json" --to=json-exact "$CONVERT/nested.values"
expect_json "a nested value, compact" "$CONVERT/nested.compact.json" --to=json-exact --compact "$CONVERT/nested.values"
expect_json "a number is on one line" "$CONVERT/leaf.json" --to=json-exact "$CONVERT/leaf.values"
expect_json "the postfix ? is written as Some with -x" "$CONVERT/query.json" --to=json-exact -x "$CONVERT/query.values"
expect_convert_error "the postfix ? needs -x" 1 "ERROR: Postfix '?' is experimental" --to=json-exact \
    "$CONVERT/query.values"

# Only the first value is converted, and one warning says so. The standard output is only the JSON.
actual=0
"$CONSTRUCTO" convert --to=json-exact "$CONVERT/three.values" >"$conv_dir/out" 2>"$conv_dir/err" || actual=$?
warning="WARNING: More values follow; only the first was converted, so use --to-array for all ("
if [[ "$actual" -eq 0 && "$(cat "$conv_dir/out")" == '{"nat": "1"}' && "$(wc -l <"$conv_dir/err")" -eq 1 \
    && "$(cat "$conv_dir/err")" == "$warning"*"three.values, line 2, col 1)" ]]; then
    pass "convert: more values give one warning and the first is converted"
else
    fail "convert: more values give one warning (exit $actual, err '$(cat "$conv_dir/err")')"
fi
expect_json "--to-array takes every value and does not warn" "$CONVERT/three.json" --to=json-exact --to-array \
    "$CONVERT/three.values"
expect_json "a file with no values is skipped" "$CONVERT/leaf.json" --to=json-exact "$CONVERT/empty.values" \
    "$CONVERT/leaf.values"
expect_json "no values with --to-array is an empty array" "$CONVERT/empty.json" --to=json-exact --to-array \
    "$CONVERT/empty.values"

# Errors give no output at all, even where there is a value that could be converted.
expect_convert_error "no value is an error" 1 "ERROR: No value to convert" --to=json-exact "$CONVERT/empty.values"
expect_convert_error "a types file is an error" 1 "ERROR: *types-file.types is a types file; convert reads values" \
    --to=json-exact "$CONVERT/leaf.values" "$CONVERT/types-file.types"
expect_convert_error "a syntax error is an error" 1 "ERROR: " --to=json-exact "$CONVERT/leaf.values" \
    "$CONVERT/syntax-error.values"
expect_convert_error "a duplicate field is an error" 1 "ERROR: Duplicate field a" --to=json-exact \
    "$CONVERT/duplicate-field.values"
expect_convert_error "a duplicate argument is an error" 1 "ERROR: Argument a is given more than once in Span" \
    --to=json-exact "$CONVERT/duplicate-argument.values"
expect_convert_error "an earlier syntax error wins over a later unreadable input" 1 "ERROR: " --to=json-exact \
    "$CONVERT/syntax-error.values" "$conv_dir/missing.values"
expect_convert_error "an unreadable input is a usage error" 2 "ERROR: Cannot read " --to=json-exact \
    "$conv_dir/missing.values" "$CONVERT/syntax-error.values"
expect_convert_error "no inputs is a usage error" 2 "error: " --to=json-exact

# The format is matched exactly, and there is no bare json.
expect_convert_error "no --to is a usage error" 2 "error: " "$CONVERT/leaf.values"
expect_convert_error "json is not a format" 2 "ERROR: json is not a format; choose json-exact" --to=json \
    "$CONVERT/leaf.values"
expect_convert_error "the format is case-sensitive" 2 "ERROR: JSON-Exact is not a format" --to=JSON-Exact \
    "$CONVERT/leaf.values"
expect_exit "convert: check's options are not accepted" 2 convert --to=json-exact --no-prelude "$CONVERT/leaf.values"
expect_exit "convert: --config must be valid" 2 --config "$conv_dir/missing.toml" convert --to=json-exact \
    "$CONVERT/leaf.values"

# `jsonl-exact` writes every value as a compact line, and the lines are the elements of the array of --to-array.
expect_json "jsonl-exact: every kind of value is a line" "$CONVERT/kinds.jsonl" --to=jsonl-exact "$CONVERT/kinds.values"
expect_json "jsonl-exact: every value is converted, with no warning" "$CONVERT/three.jsonl" --to=jsonl-exact \
    "$CONVERT/three.values"
expect_json "jsonl-exact: --compact makes no difference" "$CONVERT/three.jsonl" --to=jsonl-exact --compact \
    "$CONVERT/three.values"
expect_json "jsonl-exact: -x writes the postfix ? as Some" "$CONVERT/query.jsonl" --to=jsonl-exact -x \
    "$CONVERT/query.values"
expect_json "jsonl-exact: no values is no output" /dev/null --to=jsonl-exact "$CONVERT/empty.values"
expect_json "jsonl-exact: the values of several inputs are in order" "$CONVERT/three.jsonl" --to=jsonl-exact \
    "$CONVERT/leaf-1.values" "$CONVERT/leaf-23.values"
# The lines are written as they are converted, so the lines before an error in a value are written. An error that is
# found before any value is converted, such as one in the syntax, leaves the standard output empty.
actual=0
"$CONSTRUCTO" convert --to=jsonl-exact "$CONVERT/leaf.values" "$CONVERT/duplicate-field.values" >"$conv_dir/out" \
    2>"$conv_dir/err" || actual=$?
first="$(head -n 1 "$conv_dir/err")"
if [[ "$actual" -eq 1 && "$(cat "$conv_dir/out")" == '{"nat":"5"}' && "$first" == "ERROR: Duplicate field a"* ]]; then
    pass "jsonl-exact: the lines before an error in a value are written"
else
    fail "jsonl-exact: the lines before an error (exit $actual, out '$(cat "$conv_dir/out")', err '$first')"
fi
expect_convert_error "jsonl-exact: a syntax error is found before anything is written" 1 "ERROR: " --to=jsonl-exact \
    "$CONVERT/leaf.values" "$CONVERT/syntax-error.values"
expect_convert_error "jsonl-exact: a types file is found before anything is written" 1 "ERROR: *is a types file" \
    --to=jsonl-exact "$CONVERT/leaf.values" "$CONVERT/types-file.types"

# A reader that goes away, as `head` does, ends the run with exit code 2 and no message, and not with a panic.
seq 1 20000 | sed 's/$/;/' >"$conv_dir/long.values"
code_file="$conv_dir/code"
( "$CONSTRUCTO" convert --to=jsonl-exact "$conv_dir/long.values" 2>"$conv_dir/err"; echo $? >"$code_file" ) \
    | head -n 1 >"$conv_dir/out" || true
if [[ "$(cat "$code_file")" -eq 2 && "$(cat "$conv_dir/out")" == '{"nat":"1"}' && ! -s "$conv_dir/err" ]]; then
    pass "jsonl-exact: a reader that has gone ends the run quietly with exit code 2"
else
    fail "jsonl-exact: a reader that has gone (exit $(cat "$code_file"), err '$(head -c 200 "$conv_dir/err")')"
fi
rm -f "$conv_dir/long.values" "$code_file"
expect_convert_error "jsonl-exact: --to-array does not apply" 2 "ERROR: --to-array does not apply to jsonl-exact" \
    --to=jsonl-exact --to-array "$CONVERT/leaf.values"
expect_convert_error "jsonl is not a format" 2 "ERROR: jsonl is not a format; choose json-exact, jsonl-exact" \
    --to=jsonl "$CONVERT/leaf.values"

# `json-typed` writes natural JSON, and the values are checked against the types first.
TYPED="$CONVERT/typed"
expect_json "json-typed: every kind of value, pretty" "$TYPED/kinds.json" --to=json-typed --to-array \
    "$TYPED/schema.types" "$TYPED/kinds.values"
expect_json "json-typed: every kind of value, compact" "$TYPED/kinds.compact.json" --to=json-typed --to-array \
    --compact "$TYPED/schema.types" "$TYPED/kinds.values"
expect_json "json-typed: no values with --to-array is an empty array" "$CONVERT/empty.json" --to=json-typed --to-array \
    "$TYPED/schema.types" "$CONVERT/empty.values"
expect_convert_error "json-typed: no value is an error" 1 "ERROR: No value to convert" --to=json-typed \
    "$TYPED/schema.types"
expect_convert_error "json-typed: a type error is an error, and nothing is written" 1 \
    "ERROR: Expected Float but found String" --to=json-typed "$TYPED/schema.types" "$TYPED/type-error.values"
expect_convert_error "json-typed: an unknown constructor is an error" 1 "ERROR: Unknown constructor Nope" \
    --to=json-typed "$TYPED/schema.types" "$TYPED/unknown-constructor.values"
expect_convert_error "json-typed: a value too large to write is an error, and not a hang" 1 \
    "ERROR: The value is too large to convert" --to=json-typed --allow-refs "$TYPED/explosion.types" \
    "$TYPED/explosion.values"
expect_convert_error "json-typed: the defaults of omitted arguments count against the size of a value" 1 \
    "ERROR: The value is too large to convert" --to=json-typed --allow-refs "$TYPED/default-explosion.types" \
    "$TYPED/default-explosion.values"
expect_convert_error "json-typed: Some(None) cannot be written" 1 \
    "ERROR: Some(None) cannot be written as JSON, since it would read back as None" --to=json-typed \
    "$TYPED/schema.types" "$TYPED/some-none.values"
expect_convert_error "json-typed: Some(null) cannot be written" 1 "ERROR: Some(None) cannot be written as JSON" \
    --to=json-typed "$TYPED/schema.types" "$TYPED/some-null.values"
expect_convert_error "json-typed: an error in the types is an error" 1 "ERROR: Unknown type Missing" \
    --to=json-typed "$TYPED/bad-types.types" "$TYPED/two.values"
expect_convert_error "json-typed: --no-prelude leaves out Bool and Option" 1 "ERROR: Unknown type Option" \
    --to=json-typed --no-prelude "$TYPED/schema.types" "$TYPED/two.values"
expect_convert_error "json-typed: --no-widening needs the exact type" 1 "ERROR: Expected Float but found Nat" \
    --to=json-typed --no-widening "$TYPED/schema.types" "$CONVERT/typed/widen.values"

# Only the first value is converted, and so only it is checked. The warning is the one of json-exact.
actual=0
"$CONSTRUCTO" convert --to=json-typed "$TYPED/schema.types" "$TYPED/two.values" >"$conv_dir/out" 2>"$conv_dir/err" \
    || actual=$?
warning="WARNING: More values follow; only the first was converted, so use --to-array for all ("
if [[ "$actual" -eq 0 && "$(cat "$conv_dir/out")" == '"Dot"' && "$(wc -l <"$conv_dir/err")" -eq 1 \
    && "$(cat "$conv_dir/err")" == "$warning"*"two.values, line 2, col 1)" ]]; then
    pass "json-typed: more values give one warning and the first is converted"
else
    fail "json-typed: more values give one warning (exit $actual, err '$(cat "$conv_dir/err")')"
fi
actual=0
"$CONSTRUCTO" convert --to=json-typed "$TYPED/schema.types" "$TYPED/late-error.values" >"$conv_dir/out" 2>/dev/null \
    || actual=$?
if [[ "$actual" -eq 0 && "$(cat "$conv_dir/out")" == *'"Circle"'* ]]; then
    pass "json-typed: a type error in a value that is not converted is not found"
else
    fail "json-typed: a type error in a value that is not converted (exit $actual, out '$(cat "$conv_dir/out")')"
fi
expect_convert_error "json-typed: the same error is found with --to-array" 1 "ERROR: Expected Float but found String" \
    --to=json-typed --to-array "$TYPED/schema.types" "$TYPED/late-error.values"

# `jsonl-typed` writes each value as a line of `json-typed`, as it is converted. Every value is converted, so every
# value is type-checked.
expect_json "jsonl-typed: every kind of value is a line" "$TYPED/kinds.jsonl" --to=jsonl-typed "$TYPED/schema.types" \
    "$TYPED/kinds.values"
expect_json "jsonl-typed: every value is converted, with no warning" "$TYPED/two.jsonl" --to=jsonl-typed \
    "$TYPED/schema.types" "$TYPED/two.values"
expect_json "jsonl-typed: --compact makes no difference" "$TYPED/two.jsonl" --to=jsonl-typed --compact \
    "$TYPED/schema.types" "$TYPED/two.values"
expect_json "jsonl-typed: no values is no output" /dev/null --to=jsonl-typed "$TYPED/schema.types"
expect_json "jsonl-typed: a Nat is written for the Float that it fills" "$TYPED/widen.jsonl" --to=jsonl-typed \
    --widening "$TYPED/schema.types" "$TYPED/widen.values"
expect_convert_error "jsonl-typed: --no-widening needs the exact type" 1 "ERROR: Expected Float but found Nat" \
    --to=jsonl-typed --no-widening "$TYPED/schema.types" "$TYPED/widen.values"
actual=0
"$CONSTRUCTO" convert --to=jsonl-typed "$TYPED/schema.types" "$TYPED/late-error.values" >"$conv_dir/out" \
    2>"$conv_dir/err" || actual=$?
first="$(head -n 1 "$conv_dir/err")"
if [[ "$actual" -eq 1 && "$(cat "$conv_dir/out")" == '{"Circle":{"r":1.0}}' \
    && "$first" == "ERROR: Expected Float but found String"* ]]; then
    pass "jsonl-typed: the lines before a type error are written"
else
    fail "jsonl-typed: the lines before a type error (exit $actual, out '$(cat "$conv_dir/out")', err '$first')"
fi
expect_convert_error "jsonl-typed: an error in the types is found before anything is written" 1 \
    "ERROR: Unknown type Missing" --to=jsonl-typed "$TYPED/bad-types.types" "$TYPED/two.values"
expect_convert_error "jsonl-typed: an unknown constructor is an error" 1 "ERROR: Unknown constructor Nope" \
    --to=jsonl-typed "$TYPED/schema.types" "$TYPED/unknown-constructor.values"
expect_convert_error "jsonl-typed: Some(None) cannot be written" 1 "ERROR: Some(None) cannot be written as JSON" \
    --to=jsonl-typed "$TYPED/schema.types" "$TYPED/some-none.values"
expect_convert_error "jsonl-typed: --to-array does not apply" 2 "ERROR: --to-array does not apply to jsonl-typed" \
    --to=jsonl-typed --to-array "$TYPED/schema.types" "$TYPED/two.values"
code_file="$conv_dir/code"
seq 1 20000 | sed 's/$/;/' >"$conv_dir/long.values"
( "$CONSTRUCTO" convert --to=jsonl-typed "$conv_dir/long.values" 2>"$conv_dir/err"; echo $? >"$code_file" ) \
    | head -n 1 >"$conv_dir/out" || true
if [[ "$(cat "$code_file")" -eq 2 && "$(cat "$conv_dir/out")" == '1' && ! -s "$conv_dir/err" ]]; then
    pass "jsonl-typed: a reader that has gone ends the run quietly with exit code 2"
else
    fail "jsonl-typed: a reader that has gone (exit $(cat "$code_file"), err '$(head -c 200 "$conv_dir/err")')"
fi
rm -f "$conv_dir/long.values" "$code_file"

# The options for the types are for json-typed, and are refused for the formats that need no types.
expect_convert_error "json-exact: --no-prelude does not apply" 2 "ERROR: --no-prelude does not apply to json-exact" \
    --to=json-exact --no-prelude "$CONVERT/leaf.values"
expect_convert_error "jsonl-exact: --project does not apply" 2 "ERROR: --project does not apply to jsonl-exact" \
    --to=jsonl-exact --project="$conv_dir" "$CONVERT/leaf.values"
expect_convert_error "json-exact: --dev does not apply" 2 "ERROR: --dev does not apply to json-exact" --to=json-exact \
    --dev "$CONVERT/leaf.values"

# The standard input holds the values, and the types come from the files.
out="$(printf 'Dot;\n' | "$CONSTRUCTO" convert --to=json-typed --compact --stdin "$TYPED/schema.types" 2>&1)" || true
if [[ "$out" == '"Dot"' ]]; then pass "json-typed: --stdin"; else fail "json-typed: --stdin (got '$out')"; fi

# The settings file: the keys of [check] that apply, which an option overrides.
printf '[check]\nwidening = false\n' >"$conv_dir/settings.toml"
expect_convert_error "json-typed: widening = false in the settings file" 1 "ERROR: Expected Float but found Nat" \
    --config "$conv_dir/settings.toml" --to=json-typed "$TYPED/schema.types" "$TYPED/widen.values"
expect_json "json-typed: an option overrides the settings file" "$TYPED/widen.json" --config "$conv_dir/settings.toml" \
    --to=json-typed --widening --compact "$TYPED/schema.types" "$TYPED/widen.values"

# A project gives the types. A name is written as the declaring enum has it, without the project. A project that is
# imported from a folder needs its lock, or --dev.
proj="$conv_dir/proj"
mkdir -p "$proj/lib" "$proj/app"
printf '[project]\nname = "com.example.lib"\ninclude = ["*.types"]\n' >"$proj/lib/constructo.toml"
printf 'enum Shape = Circle { r: Float }, Dot;\n' >"$proj/lib/shapes.types"
printf '[project]\nname = "com.example.app"\ninclude = ["*.types"]\n\n[[import]]\nproject-name = "com.example.lib"\n' \
    >"$proj/app/constructo.toml"
printf 'group = "lib"\npath = "../lib"\n' >>"$proj/app/constructo.toml"
printf 'enum Scene = Scene { shape: lib::Shape };\n' >"$proj/app/scene.types"
printf 'Scene { shape: lib::Circle { r: 2 } };\nlib::Dot;\n' >"$proj/v.values"
printf '{"Scene":{"shape":{"Circle":{"r":2.0}}}}\n' >"$proj/expected-first.json"
printf '[{"Scene":{"shape":{"Circle":{"r":2.0}}}},"Dot"]\n' >"$proj/expected-all.json"
expect_convert_error "json-typed: a project that imports a folder needs its lock" 1 "ERROR: The project imports" \
    --to=json-typed --project="$proj/app" "$proj/v.values"
expect_json "json-typed: --dev does not need the lock" "$proj/expected-all.json" --to=json-typed --to-array --compact \
    --dev --project="$proj/app" "$proj/v.values"
"$CONSTRUCTO" project update "$proj/app" >/dev/null 2>&1
expect_json "json-typed: a project with a lock" "$proj/expected-all.json" --to=json-typed --to-array --compact \
    --project="$proj/app" "$proj/v.values"
actual=0
"$CONSTRUCTO" convert --to=json-typed --compact --project="$proj/app" "$proj/v.values" >"$conv_dir/out" 2>/dev/null \
    || actual=$?
if [[ "$actual" -eq 0 && "$(cat "$conv_dir/out")" == "$(cat "$proj/expected-first.json")" ]]; then
    pass "json-typed: the first value of a project"
else
    fail "json-typed: the first value of a project (exit $actual, out '$(cat "$conv_dir/out")')"
fi
rm -f "$proj/app/constructo.lock" "$proj/app/constructo.toml" "$proj/app/scene.types" "$proj/lib/constructo.toml" \
    "$proj/lib/shapes.types" "$proj/v.values" "$proj/expected-first.json" "$proj/expected-all.json"
rmdir "$proj/app" "$proj/lib" "$proj"
rm -f "$conv_dir/settings.toml"

# `--from` reads the JSON that `--to` writes and writes the values as text. The examples are in `back/`.
BACK="$CONVERT/back"
expect_json "from json-exact: every kind of value" "$BACK/kinds.values" --from=json-exact "$CONVERT/kinds.json"
expect_json "from json-exact: --compact is a value to a line" "$BACK/kinds.compact.values" --from=json-exact \
    --compact "$CONVERT/kinds.json"
expect_json "from json-exact: the compact form is read too" "$BACK/kinds.values" --from=json-exact \
    "$CONVERT/kinds.compact.json"
expect_json "from json-exact: an object is one value" "$BACK/nested.values" --from=json-exact "$CONVERT/nested.json"
expect_json "from json-exact: a wide value is spread" "$BACK/wide.out" --from=json-exact "$BACK/wide.json"
expect_json "from json-exact: an empty array is no values" /dev/null --from=json-exact "$CONVERT/empty.json"
expect_json "from jsonl-exact: every value is a line, and blank lines are skipped" "$BACK/loose.out" \
    --from=jsonl-exact "$BACK/loose.jsonl"
expect_json "from jsonl-exact: an empty file is no values" /dev/null --from=jsonl-exact "$BACK/empty.jsonl"
expect_json "from jsonl-exact: the values of several inputs are in order" "$BACK/two-two.values" \
    --from=jsonl-exact "$BACK/two.jsonl" "$BACK/two.jsonl"

# A round trip through the other direction gives the values again, compactly.
out="$("$CONSTRUCTO" convert --to=jsonl-exact "$CONVERT/kinds.values" \
    | "$CONSTRUCTO" convert --from=jsonl-exact --compact --stdin 2>&1)" || true
if [[ "$out" == "$(cat "$BACK/kinds.compact.values")" ]]; then
    pass "from jsonl-exact: a round trip through --to=jsonl-exact"
else
    fail "from jsonl-exact: a round trip through --to=jsonl-exact (got '$out')"
fi
out="$("$CONSTRUCTO" convert --to=json-exact --to-array "$CONVERT/kinds.values" \
    | "$CONSTRUCTO" convert --from=json-exact --compact --stdin 2>&1)" || true
if [[ "$out" == "$(cat "$BACK/kinds.compact.values")" ]]; then
    pass "from json-exact: a round trip through --to=json-exact"
else
    fail "from json-exact: a round trip through --to=json-exact (got '$out')"
fi

# A JSON document is read whole before any value is written, so a problem in it leaves the standard output empty.
expect_convert_error "from json-exact: a syntax error writes nothing" 1 "ERROR: Expected \",\" or \"]\"" \
    --from=json-exact "$BACK/truncated.json"
expect_convert_error "from json-exact: an empty file is an error" 1 "ERROR: Expected a JSON value" \
    --from=json-exact "$BACK/blank.json"
expect_convert_error "from json-exact: a number in place of a string" 1 \
    "ERROR: Expected a string for \"nat\"; a number is written as a string" --from=json-exact "$BACK/number.json"
expect_convert_error "from json-exact: text of the wrong kind of number" 1 "ERROR: The text \"+5\" is not a nat" \
    --from=json-exact "$BACK/wrong-kind.json"
expect_convert_error "from json-exact: a json-typed file is not json-exact" 1 "ERROR: Unknown key \"Circle\"" \
    --from=json-exact "$BACK/typed.json"
if grep -q -- "--to=json-typed" "$conv_dir/err"; then
    pass "convert: the error for a json-typed file says what it may be"
else
    fail "convert: the error for a json-typed file says what it may be"
fi

# The values before the one that is wrong are written, and the run stops there.
expect_before_error() {
    local name="$1" written="$2" message="$3"; shift 3
    local actual=0
    "$CONSTRUCTO" convert "$@" >"$conv_dir/out" 2>"$conv_dir/err" || actual=$?
    local first
    first="$(head -n 1 "$conv_dir/err")"
    if [[ "$actual" -eq 1 && "$(cat "$conv_dir/out")" == "$written" && "$first" == $message* ]]; then
        pass "convert: $name"
    else
        fail "convert: $name (exit $actual, stdout '$(cat "$conv_dir/out")', first error '$first')"
    fi
}
expect_before_error "from json-exact: the values before a bad element are written" "1;" \
    "ERROR: The text \"x\" is not a nat (*late-error.json, line 1, col 24)" --from=json-exact "$BACK/late-error.json"
expect_before_error "from jsonl-exact: an array on a line is an error at that line" "1;" \
    "ERROR: Expected an object for a value (*array-line.jsonl, line 3, col 1)" --from=jsonl-exact \
    "$BACK/array-line.jsonl"
expect_before_error "from jsonl-exact: a line that stops half way ends the run there" "1;" \
    "ERROR: Expected a JSON value (*half.jsonl, line 2, col 9)" --from=jsonl-exact "$BACK/half.jsonl"

# An input that cannot be read ends the run with exit code 2, after the values of the inputs before it.
actual=0
"$CONSTRUCTO" convert --from=jsonl-exact "$BACK/two.jsonl" "$conv_dir/missing.jsonl" >"$conv_dir/out" \
    2>"$conv_dir/err" || actual=$?
first="$(head -n 1 "$conv_dir/err")"
if [[ "$actual" -eq 2 && "$(cat "$conv_dir/out")" == $'1;\n2;' && "$first" == "ERROR: Cannot read"* ]]; then
    pass "from jsonl-exact: an unreadable input ends the run with exit code 2 after the earlier values"
else
    fail "from jsonl-exact: an unreadable input (exit $actual, out '$(cat "$conv_dir/out")')"
fi
# A line is converted as it comes, so a file that stops being readable part way gives the lines before that point.
printf '{"nat": "1"}\n\xff\n' >"$conv_dir/not-utf8.jsonl"
actual=0
"$CONSTRUCTO" convert --from=jsonl-exact "$conv_dir/not-utf8.jsonl" >"$conv_dir/out" 2>"$conv_dir/err" || actual=$?
first="$(head -n 1 "$conv_dir/err")"
if [[ "$actual" -eq 2 && "$(cat "$conv_dir/out")" == "1;" && "$first" == "ERROR: Cannot read"* ]]; then
    pass "from jsonl-exact: an input that stops being readable part way gives the lines before that point"
else
    fail "from jsonl-exact: an input that stops being readable part way (exit $actual, out '$(cat "$conv_dir/out")')"
fi
# The first value arrives while the writer of the pipe is still open, so nothing waits for the end of the input.
out="$( (printf '{"nat": "1"}\n'; sleep 3) \
    | timeout 1 "$CONSTRUCTO" convert --from=jsonl-exact --stdin 2>/dev/null )" || true
if [[ "$out" == "1;" ]]; then
    pass "from jsonl-exact: a line from the standard input is written before the end of the input"
else
    fail "from jsonl-exact: a line from the standard input is written before the end of the input (got '$out')"
fi
out="$(printf '{"nat": "7"}\n' \
    | "$CONSTRUCTO" convert --from=jsonl-exact --stdin "$BACK/two.jsonl" 2>/dev/null)" || true
if [[ "$out" == $'1;\n2;\n7;' ]]; then
    pass "from jsonl-exact: the standard input comes after the files"
else
    fail "from jsonl-exact: the standard input comes after the files (got '$out')"
fi

expect_convert_error "from: --to and --from together" 2 "error: the argument '--to <FORMAT>' cannot be used with" \
    --to=json-exact --from=json-exact "$BACK/two.jsonl"
expect_convert_error "from: neither --to nor --from" 2 "error: the following required arguments were not provided" \
    "$BACK/two.jsonl"
expect_convert_error "from: json-typed cannot be read back yet" 2 "ERROR: json-typed cannot be read back yet" \
    --from=json-typed "$BACK/two.jsonl"
expect_convert_error "from: a bare json is not a format" 2 "ERROR: json is not a format" --from=json "$BACK/two.jsonl"
expect_convert_error "from: --to-array does not apply" 2 "ERROR: --to-array does not apply with --from" \
    --from=json-exact --to-array "$BACK/two.jsonl"
expect_convert_error "from: -x does not apply" 2 "ERROR: --experimental does not apply with --from" \
    --from=json-exact -x "$BACK/two.jsonl"
expect_convert_error "from: the typing options do not apply" 2 "ERROR: --widening does not apply with --from" \
    --from=json-exact --widening "$BACK/two.jsonl"

# The standard input is read after the files.
out="$(printf '7;\n' | "$CONSTRUCTO" convert --to=json-exact --compact --stdin 2>&1)" || true
if [[ "$out" == '{"nat":"7"}' ]]; then pass "convert: --stdin"; else fail "convert: --stdin (got '$out')"; fi
out="$(printf '7;\n' | "$CONSTRUCTO" convert --to=json-exact --compact --stdin "$CONVERT/leaf.values" \
    2>/dev/null)" || true
if [[ "$out" == '{"nat":"5"}' ]]; then
    pass "convert: the files come before the standard input"
else
    fail "convert: the files come before the standard input (got '$out')"
fi
rm -f "$conv_dir/out" "$conv_dir/err" "$conv_dir/not-utf8.jsonl"
rmdir "$conv_dir"
echo ""

TOTAL=$(( PASS + FAIL ))
echo "Results: $PASS/$TOTAL passed"
if [[ $FAIL -gt 0 ]]; then
    echo "FAILED: $FAIL test(s) failed"
    exit 1
fi
