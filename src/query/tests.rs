//! The examples of the query task, which drive the implementation. Each runs a query over the values of a text and
//! compares what is written, one compact value to a line, or the error.

use super::*;
use crate::values_text::render;

/// Runs a query over the values in `input`, and gives the output with each value compacted and on a line, or the
/// first error with what was written before it.
fn query(input: &str, text: &str) -> Result<String, String> {
    let expr = prepare(text).map_err(|d| format!("before any value: {}", d))?;
    let input = input.to_string();
    let source: Box<dyn FnOnce() -> Result<Source, String>> = Box::new(move || {
        Ok(Source {
            name: "in.values".to_string(),
            text: input,
        })
    });
    let stream = stream_of(std::iter::once(source), Experimental::default());
    let mut written = String::new();
    let result = run(&expr, stream, &mut |v| {
        written.push_str(&render(v, true));
        Ok(())
    });
    match result {
        Ok(()) => Ok(written.trim_end().replace(";\n", "; ")),
        Err(RunError::Failed(Failure::Error(d))) => Err(format!("{}after: {}", d, written.replace(";\n", "; "))),
        Err(RunError::Failed(Failure::Input(m))) => Err(m),
        Err(RunError::Write(e)) => Err(e.to_string()),
        Err(RunError::Query(d)) => Err(format!("before any value: {}", d)),
    }
}

fn ok(input: &str, text: &str) -> String {
    query(input, text).unwrap_or_else(|e| panic!("{} failed: {}", text, e))
}

fn err(input: &str, text: &str) -> String {
    match query(input, text) {
        Ok(out) => panic!("{} gave {}", text, out),
        Err(e) => e,
    }
}

// Extraction.

#[test]
fn e01_pass_the_input_through() {
    assert_eq!(ok(r#"1; "a"; Quit;"#, "@values"), r#"1; "a"; Quit;"#);
}

#[test]
fn e02_delete_the_whole_input() {
    assert_eq!(ok("1; 2; 3;", "@values.take(0)"), "");
}

#[test]
fn e03_the_first_item() {
    assert_eq!(ok("10; 20; 30;", "@values.take(1)"), "10;");
    assert_eq!(ok("10; 20; 30;", "@values.next()"), "Some(10);");
    assert_eq!(ok("", "@values.take(1)"), "");
    assert_eq!(ok("", "@values.next()"), "None;");
}

#[test]
fn e04_the_third_item() {
    assert_eq!(ok("10; 20; 30;", "@values.skip(2).take(1)"), "30;");
    assert_eq!(ok("10; 20; 30;", "@values.nth(2)"), "Some(30);");
    assert_eq!(ok("10; 20;", "@values.skip(2).take(1)"), "");
    assert_eq!(ok("10; 20;", "@values.nth(2)"), "None;");
}

#[test]
fn e05_a_field_of_records() {
    assert_eq!(ok("{ x: 1, y: 2 }; { x: 3, y: 4 };", "@values.map(|r| r.x)"), "1; 3;");
}

#[test]
fn e06_a_pair_of_fields() {
    assert_eq!(
        ok(
            "{ x: 1, y: 2, z: 9 }; { x: 3, y: 4, z: 9 };",
            "@values.map(|r| { x: r.x, y: r.y })"
        ),
        "{ x: 1, y: 2 }; { x: 3, y: 4 };"
    );
}

const ARRAYS: &str = "[1, 2]; []; [5, 6, 7];";

#[test]
fn e07_lengths_of_arrays() {
    assert_eq!(ok(ARRAYS, "@values.map(|a| a.len())"), "2; 0; 3;");
}

#[test]
fn e08_first_elements() {
    let e = err(ARRAYS, "@values.map(|a| a[0])");
    assert!(
        e.contains("out of range") && e.contains("(in.values, line 1, col 9)") && e.ends_with("after: 1; "),
        "{}",
        e
    );
    assert_eq!(ok(ARRAYS, "@values.filter_map(|a| a.first())"), "1; 5;");
}

#[test]
fn e09_third_elements() {
    assert_eq!(ok(ARRAYS, "@values.filter_map(|a| a.get(2))"), "7;");
}

const PAIRS: &str = "[Pair { left: true, right: false }, Pair { left: false, right: true }]; [];";

#[test]
fn e10_fields_from_arrays_of_records() {
    assert_eq!(
        ok(PAIRS, "@values.flat_map(|ps| ps.iter().map(|p| p.right))"),
        "false; true;"
    );
    assert_eq!(
        ok(PAIRS, "@values.map(|ps| ps.iter().map(|p| p.right).collect())"),
        "[false, true]; [];"
    );
}

const RECORDS: &str = "[Record(true, right: false), Record(false, false)];";

#[test]
fn e11_positional_and_named_arguments() {
    assert_eq!(
        ok(RECORDS, "@values.flat_map(|rs| rs.iter().map(|r| r.0))"),
        "true; false;"
    );
    let e = err(RECORDS, "@values.flat_map(|rs| rs.iter().map(|r| r.right))");
    assert!(
        e.contains("supply the types") && e.contains("line 1, col 30") && e.ends_with("after: false; "),
        "{}",
        e
    );
}

// Patterns and constructors.

const MOVES: &str = "Move { x: 1, y: 2 }; Quit; Move { x: 3, y: 4 };";

#[test]
fn e12_filter_match_skips_other_constructors() {
    assert_eq!(ok(MOVES, "@values.filter_match(|Move { x, .. }| x)"), "1; 3;");
}

#[test]
fn e13_field_shorthand_after_a_constructor_name() {
    assert_eq!(
        ok(MOVES, "@values.filter_match(|Move { x, y, .. }| Point { x, y })"),
        "Point { x: 1, y: 2 }; Point { x: 3, y: 4 };"
    );
    let e = err(MOVES, "@values.filter_match(|Move { x, .. }| { x })");
    assert!(
        e.starts_with("before any value") && e.contains("write x for the value, or { x: x }"),
        "{}",
        e
    );
}

#[test]
fn e14_matches() {
    assert_eq!(ok(MOVES, "@values.filter(|v| matches!(v, Quit)).count()"), "1;");
}

#[test]
fn e15_a_refutable_pattern_in_map_is_refused() {
    let e = err(MOVES, "@values.map(|Move { x, .. }| x)");
    assert!(
        e.starts_with("before any value") && e.contains("must always match"),
        "{}",
        e
    );
}

#[test]
fn e16_untyped_names_match_by_exact_text() {
    assert_eq!(
        ok(
            "shapes::Circle { r: 1.0 }; Circle { r: 2.0 };",
            "@values.filter_match(|Circle { r }| r)"
        ),
        "2.0;"
    );
}

// Numbers.

#[test]
fn e17_a_literal_adapts() {
    assert_eq!(
        ok("{ y: 1.5 }; { y: -2.0 };", "@values.filter(|m| m.y > 0.0).count()"),
        "1;"
    );
}

#[test]
fn e18_mixed_kinds_need_a_cast() {
    let e = err("{ y: 1.5 }; { y: 5 };", "@values.filter(|m| m.y > 0.0).count()");
    assert!(
        e.contains("5 is a Nat") && e.contains("`as Float`") && e.contains("line 1, col 18"),
        "{}",
        e
    );
}

#[test]
fn e19_the_cast_works_on_every_spelling() {
    assert_eq!(
        ok(
            "{ y: 1.5 }; { y: 5 };",
            "@values.filter(|m| m.y as Float > 0.0).count()"
        ),
        "2;"
    );
}

#[test]
fn e21_a_narrowing_cast_that_changes_the_number() {
    let e = err("-1;", "@values.map(|n| n as Nat)");
    assert!(
        e.contains("negative") && e.contains("(in.values, line 1, col 1)"),
        "{}",
        e
    );
    assert_eq!(ok("5.0; +5;", "@values.map(|n| n as Nat)"), "5; 5;");
}

#[test]
fn e22_to_e28_arithmetic() {
    assert_eq!(ok("{ port: 8080 };", "@values.map(|c| c.port + 1)"), "8081;");
    assert!(err("3;", "@values.map(|n| n - 5)").contains("below zero"));
    assert_eq!(ok("3;", "@values.map(|n| n as Int - 5)"), "-2;");
    assert_eq!(
        ok("18446744073709551615;", "@values.map(|n| n + 1)"),
        "18446744073709551616;"
    );
    assert_eq!(ok("7;", "@values.map(|n| n / 2)"), "3;");
    assert_eq!(ok("7;", "@values.map(|n| n % 2)"), "1;");
    assert_eq!(ok("0.1;", "@values.map(|x| x + 0.2)"), "0.30000000000000004;");
    assert_eq!(ok("1.5;", "@values.map(|x| x * 2.0)"), "3.0;");
}

#[test]
fn e29_untouched_literals_keep_their_text() {
    assert_eq!(ok("{ a: 1.50, b: +5 };", "@values.map(|r| r)"), "{ a: 1.50, b: +5 };");
}

// Transformation.

#[test]
fn e30_e31_struct_update() {
    assert_eq!(
        ok(
            r#"Config { name: "h", port: 80 };"#,
            "@values.map(|c| Config { port: 8080, ..c })"
        ),
        r#"Config { name: "h", port: 8080 };"#
    );
    assert_eq!(
        ok("{ a: 1, b: 2 };", "@values.map(|r| { b: 3, ..r })"),
        "{ a: 1, b: 3 };"
    );
}

#[test]
fn e32_to_e36_update() {
    assert_eq!(
        ok(
            r#"{ server: { tls: { port: 1 }, host: "x" } };"#,
            "@values.map(|c| c.update(|c| c.server.tls.port = 443))"
        ),
        r#"{ server: { tls: { port: 443 }, host: "x" } };"#
    );
    assert_eq!(
        ok(
            "{ a: 1, b: { c: 2 } };",
            "@values.map(|v| v.update(|v| v.a = 10).update(|v| v.b.c = 20))"
        ),
        "{ a: 10, b: { c: 20 } };"
    );
    assert_eq!(
        ok("{ count: 4 };", "@values.map(|v| v.update(|v| v.count += 1))"),
        "{ count: 5 };"
    );
    assert_eq!(
        ok("{ items: [1, 2, 3] };", "@values.map(|v| v.update(|v| v.items[1] = 9))"),
        "{ items: [1, 9, 3] };"
    );
    let e = err("{ a: 1 };", "@values.map(|v| v.update(|v| v.z = 1))");
    assert!(
        e.contains("no field z") && e.contains("(in.values, line 1, col 1)"),
        "{}",
        e
    );
}

#[test]
fn e38_e39_collect() {
    assert_eq!(
        ok("[1, 5, 2, 8];", "@values.map(|a| a.iter().filter(|n| n > 3).collect())"),
        "[5, 8];"
    );
    assert_eq!(ok("1; 2; 3;", "@values.collect()"), "[1, 2, 3];");
}

// The settled questions.

#[test]
fn branches_must_have_a_common_type() {
    let e = err(
        "{ ok: true };",
        r#"@values.map(|v| if v.ok then 1 else "failed" endif)"#,
    );
    assert!(
        e.starts_with("before any value") && e.contains("no common type"),
        "{}",
        e
    );
    // What comes from the data is checked as it arrives.
    assert_eq!(
        ok(
            r#"{ ok: false, x: "a" };"#,
            r#"@values.map(|v| if v.ok then v.x else "failed" endif)"#
        ),
        r#""failed";"#
    );
    let e = err(
        "{ ok: true, x: 1 };",
        r#"@values.map(|v| if v.ok then v.x else "failed" endif)"#,
    );
    assert!(e.contains("no common type"), "{}", e);
}

#[test]
fn collect_needs_a_common_type() {
    let e = err(r#"1; "a";"#, "@values.collect()");
    assert!(e.contains("common type") && e.contains("line 1, col 4"), "{}", e);
    assert_eq!(ok(r#"1; "a"; Quit;"#, "@values.count()"), "3;");
}

#[test]
fn if_in_both_forms_and_match() {
    assert_eq!(
        ok(
            "1; 5;",
            "@values.map(|n| if n > 3 then { big: n } else { big: 0 } endif)"
        ),
        "{ big: 0 }; { big: 5 };"
    );
    assert_eq!(ok("1; 5;", "@values.map(|n| if n > 3 { n } else { 0 })"), "0; 5;");
    assert_eq!(
        ok(
            "Circle { r: 2.0 }; Rect { w: 2.0, h: 3.0 }; Quit;",
            "@values.map(|s| match s with Circle { r } => r * r, Rect { w, h } => w * h, _ => 0.0, endmatch)"
        ),
        "4.0; 6.0; 0.0;"
    );
    let e = err("Quit;", "@values.map(|s| match s { Circle { r } => r })");
    assert!(e.contains("No arm"), "{}", e);
}

#[test]
fn strings() {
    assert_eq!(ok(r#""héllo";"#, "@values.map(|s| s.len())"), "6;");
    assert_eq!(ok(r#""héllo";"#, "@values.map(|s| s.char_strings().count())"), "5;");
    assert!(err(r#""a";"#, "@values.map(|s| s.chars().count())").contains("char_strings"));
    assert_eq!(ok(r#""a,b";"#, "@values.flat_map(|s| s.split(\",\"))"), r#""a"; "b";"#);
    assert_eq!(ok(r#""ab";"#, r#"@values.map(|s| s + "c")"#), r#""abc";"#);
}

#[test]
fn equality_and_booleans() {
    assert_eq!(ok("1.50; 2.0;", "@values.filter(|x| x == 1.5).count()"), "1;");
    assert_eq!(ok("{ a: 1, b: 2 };", "@values.map(|r| r == { b: 2, a: 1 })"), "true;");
    assert_eq!(ok("true; True; false;", "@values.filter(|b| b).count()"), "2;");
    assert!(err("1;", "@values.filter(|n| n)").contains("Expected a Bool"));
}

#[test]
fn values_once_and_outside_closures() {
    assert!(err("1;", "@values.map(|v| @values.count())").contains("inside a closure"));
}

#[test]
fn a_const_reference_needs_the_types() {
    let e = err("origin;", "@values.map(|p| p.x)");
    assert!(
        e.contains("origin is a const") && e.contains("supply the types"),
        "{}",
        e
    );
}

// Typed queries.

/// Runs a typed query over the values in `input`, with the declarations in `types`, as `query` does.
fn typed(types: &str, input: &str, text: &str, type_text: Option<&str>) -> Result<String, String> {
    let expr = prepare(text).map_err(|d| format!("before any value: {}", d))?;
    let sources = vec![
        crate::prelude_source(),
        Source {
            name: "in.types".to_string(),
            text: types.to_string(),
        },
        Source {
            name: "in.values".to_string(),
            text: input.to_string(),
        },
    ];
    let layout = crate::plain_layout(&sources);
    let names = constructor_names(&expr);
    let mut written = String::new();
    let result = crate::query_typed(
        &sources,
        &layout,
        &crate::CheckOptions::default(),
        &names,
        type_text,
        &mut |inputs| {
            run_typed(&expr, inputs, &mut |v| {
                written.push_str(&render(v, true));
                Ok(())
            })
        },
    );
    let shown = written.trim_end().replace(";\n", "; ");
    match result {
        Ok(()) => Ok(shown),
        Err(crate::QueryTypedError::Invalid(ds)) => Err(format!("invalid: {}", ds[0])),
        Err(crate::QueryTypedError::Query(d)) | Err(crate::QueryTypedError::Run(RunError::Query(d))) => {
            Err(format!("before any value: {}", d))
        }
        Err(crate::QueryTypedError::Run(RunError::Failed(Failure::Error(d)))) => Err(format!("{}after: {}", d, shown)),
        Err(crate::QueryTypedError::Run(e)) => Err(format!("{:?}", e)),
    }
}

fn typed_ok(types: &str, input: &str, text: &str) -> String {
    typed(types, input, text, None).unwrap_or_else(|e| panic!("{} failed: {}", text, e))
}

fn typed_err(types: &str, input: &str, text: &str) -> String {
    match typed(types, input, text, None) {
        Ok(out) => panic!("{} gave {}", text, out),
        Err(e) => e,
    }
}

#[test]
fn e11b_a_named_argument_typed() {
    let types = "enum R = Record(Bool, right: Bool);";
    assert_eq!(
        typed_ok(types, RECORDS, "@values.flat_map(|rs| rs.iter().map(|r| r.right))"),
        "false; false;"
    );
    assert_eq!(
        typed_ok(
            types,
            "Record(true, right: false);",
            "@values.filter_match(|Record(_, right: r)| r)"
        ),
        "false;"
    );
}

#[test]
fn e20_typed_a_nat_in_a_float_field_is_a_float() {
    let types = "enum M = M { y: Float };";
    assert_eq!(
        typed_ok(
            types,
            "M { y: 1.5 }; M { y: 5 };",
            "@values.filter(|m| m.y > 0.0).count()"
        ),
        "2;"
    );
    // The value keeps its text, and the cast to its own kind does nothing.
    assert_eq!(typed_ok(types, "M { y: 5 };", "@values.map(|m| m.y as Float)"), "5;");
    assert_eq!(typed_ok(types, "M { y: 5 };", "@values.map(|m| m.y * 2.0)"), "10.0;");
}

const SHAPES: &str = "enum Shape = Circle { r: Float }, Dot;\nenum T = T(a: Nat, b: Shape);\nconst origin = T(a: 3);";

#[test]
fn typed_defaults_consts_and_positions() {
    let input = "T(a: 1); origin; T(2, Circle { r: 1.0 });";
    assert_eq!(
        typed_ok(SHAPES, input, "@values.map(|t| t.b)"),
        "Circle { r: 0.0 }; Circle { r: 0.0 }; Circle { r: 1.0 };"
    );
    assert_eq!(typed_ok(SHAPES, input, "@values.map(|t| t.0)"), "1; 3; 2;");
    assert_eq!(
        typed_ok(SHAPES, input, "@values.map(|t| t.update(|t| t.a = 7))"),
        "T(a: 7); T(a: 7); T(7, Circle { r: 1.0 });"
    );
}

#[test]
fn typed_names_are_resolved() {
    let input = "T(a: 1, b: Dot); T(a: 2, b: Circle { r: 1.0 });";
    assert_eq!(typed_ok(SHAPES, input, "@values.filter(|t| t.b == Dot).count()"), "1;");
    let e = typed_err(SHAPES, input, "@values.map(|t| Squar)");
    assert!(
        e.starts_with("before any value") && e.contains("Unknown constructor Squar"),
        "{}",
        e
    );
}

#[test]
fn typed_values_are_checked_before_the_query_runs() {
    let e = typed_err(SHAPES, "T(a: 1); T(a: \"x\");", "@values");
    assert!(e.starts_with("invalid") && e.contains("in.values"), "{}", e);
}

#[test]
fn typed_update_must_fit() {
    let e = typed_err(SHAPES, "T(a: 1);", r#"@values.map(|t| t.update(|t| t.a = "x"))"#);
    assert!(e.contains("does not fit") && e.contains("of type Nat"), "{}", e);
}

#[test]
fn typed_with_a_type_for_every_value() {
    let e = typed(SHAPES, "T(a: 1);", "@values", Some("Shape")).unwrap_err();
    assert!(e.starts_with("invalid") && e.contains("Expected Shape"), "{}", e);
    assert_eq!(typed(SHAPES, "Dot;", "@values", Some("Shape")).unwrap(), "Dot;");
}

const CONFIG: &str = "enum Config = Config { name: String, port: Nat };";

#[test]
fn e37_typed_a_value_of_the_wrong_type_is_found_up_front() {
    let e = typed(
        CONFIG,
        r#"Config { name: "h", port: 80 };"#,
        r#"@values.map(|c| c.update(|c| c.port = "x"))"#,
        Some("Config"),
    )
    .unwrap_err();
    assert!(
        e.starts_with("before any value") && e.contains("does not fit") && e.contains("Nat"),
        "{}",
        e
    );
    assert_eq!(
        typed(
            CONFIG,
            r#"Config { name: "h", port: 80 };"#,
            "@values.map(|c| c.update(|c| c.port = 8080))",
            Some("Config")
        )
        .unwrap(),
        r#"Config { name: "h", port: 8080 };"#
    );
}

#[test]
fn typed_fields_are_checked_up_front() {
    let e = typed(CONFIG, "", "@values.map(|c| c.prot)", Some("Config")).unwrap_err();
    assert!(
        e.starts_with("before any value") && e.contains("No constructor of Config has a field or a parameter prot"),
        "{}",
        e
    );
    let e = typed(SHAPES, "", "@values.filter_match(|Circle { radius }| radius)", None).unwrap_err();
    assert!(
        e.starts_with("before any value") && e.contains("Circle has no field radius"),
        "{}",
        e
    );
}

#[test]
fn typed_patterns_must_be_of_the_type() {
    let e = typed(SHAPES, "", "@values.filter_match(|Circle { .. }| 1)", Some("T")).unwrap_err();
    assert!(e.contains("never matches a value of T"), "{}", e);
}

#[test]
fn typed_match_must_be_exhaustive() {
    let e = typed(
        SHAPES,
        "",
        "@values.map(|s| match s with Circle { r } => r, endmatch)",
        Some("Shape"),
    )
    .unwrap_err();
    assert!(
        e.starts_with("before any value") && e.contains("does not cover Dot of Shape"),
        "{}",
        e
    );
    assert_eq!(
        typed(
            SHAPES,
            "Dot; Circle { r: 2.0 };",
            "@values.map(|s| match s with Circle { r } => r, Dot => 0.0, endmatch)",
            Some("Shape")
        )
        .unwrap(),
        "0.0; 2.0;"
    );
    // Without a type for the values, the match is checked at each value.
    let e = typed_err(
        SHAPES,
        "Dot;",
        "@values.map(|s| match s with Circle { r } => r, endmatch)",
    );
    assert!(e.contains("No arm"), "{}", e);
}

// The review of the pull request.

#[test]
fn a_branch_takes_the_common_kind_of_the_branches() {
    assert_eq!(
        ok("1;", "@values.map(|v| (if true then 1 else (2.0 as Float) endif) / 2)"),
        "0.5;"
    );
    assert_eq!(
        ok("1;", "@values.map(|v| (match v { 1 => 1, _ => 2.0 as Float }) / 2)"),
        "0.5;"
    );
}

#[test]
fn an_array_literal_takes_the_common_kind_of_its_elements() {
    assert_eq!(
        ok("0;", "@values.map(|v| [1, 2.0 as Float].iter().collect())"),
        "[1.0, 2.0];"
    );
    let e = err("0;", "@values.map(|v| [1.5, 1, 0 as Nat])");
    assert!(
        e.starts_with("before any value") && e.contains("no common type"),
        "{}",
        e
    );
}

#[test]
fn methods_need_a_receiver_that_has_them() {
    let e = err("0;", "@values.map(|v| 1.trim())");
    assert!(
        e.starts_with("before any value") && e.contains("A Nat has no method trim"),
        "{}",
        e
    );
    let e = err("0;", r#"@values.map(|v| "a".first())"#);
    assert!(
        e.starts_with("before any value") && e.contains("has no method first"),
        "{}",
        e
    );
}

#[test]
fn a_predicate_cannot_give_an_iterator() {
    let e = err("0;", "@values.filter(|v| [1].iter()).count()");
    assert!(
        e.starts_with("before any value") && e.contains("must give a Bool"),
        "{}",
        e
    );
    let e = err("0;", "@values.filter_map(|v| [1].iter())");
    assert!(
        e.starts_with("before any value") && e.contains("must give an Option"),
        "{}",
        e
    );
}

#[test]
fn true_in_a_branch_fixes_a_bool() {
    let e = err(
        r#"{ ok: true, x: "s" };"#,
        "@values.map(|v| if v.ok then v.x else true endif)",
    );
    assert!(e.contains("no common type"), "{}", e);
}

#[test]
fn nesting_through_conditions_and_bindings_is_limited() {
    // The parser is allowed the stack of a main thread, which a test thread is smaller than in a debug build.
    crate::test_support::on_a_main_sized_stack(|| {
        let mut deep = "true".to_string();
        for _ in 0..300 {
            deep = format!("if {} then true else false endif", deep);
        }
        let e = err("0;", &format!("@values.map(|v| {})", deep));
        assert!(e.contains("nested more than"), "{}", e);
        let e = err(
            "0;",
            &format!("@values.map(|v| match v with {}_ => 1 endmatch)", "a @ ".repeat(300)),
        );
        assert!(e.contains("nested more than"), "{}", e);
    });
}

#[test]
fn typed_collect_and_options_keep_the_kind_of_a_number() {
    let types = "enum Unused = Unused;";
    assert_eq!(
        typed(types, "5; 2.0;", "@values.collect()", Some("Float")).unwrap(),
        "[5, 2.0];"
    );
    assert_eq!(
        typed(
            types,
            "5; 2.0;",
            "@values.collect().iter().map(|n| n / 2)",
            Some("Float")
        )
        .unwrap(),
        "2.5; 1.0;"
    );
    assert_eq!(
        typed(types, "5;", "@values.next().unwrap_or(0.0) / 2", Some("Float")).unwrap(),
        "2.5;"
    );
}

#[test]
fn typed_a_scalar_const_is_seen_into_for_arithmetic() {
    assert_eq!(typed_ok("const five = 5;", "five;", "@values.map(|n| n + 1)"), "6;");
    assert_eq!(typed_ok("const five = 5;", "five;", "@values.map(|n| n as Int)"), "+5;");
}

#[test]
fn typed_equality_uses_the_types_of_the_parts() {
    let types = "enum M = M { a: [Float], b: [Float] };";
    assert_eq!(
        typed_ok(types, "M { a: [5], b: [5.0] };", "@values.map(|m| m.a == m.b)"),
        "true;"
    );
}

#[test]
fn typed_fields_of_different_types_are_not_taken_to_be_the_first() {
    let types = "enum W<T> = W(T);\nenum Choice = A { x: W<{ a: Nat }> }, B { x: W<{ b: Nat }> };";
    assert_eq!(
        typed(
            types,
            "B { x: W({ b: 1 }) };",
            "@values.map(|v| v.x.0.b)",
            Some("Choice")
        )
        .unwrap(),
        "1;"
    );
}

#[test]
fn typed_alternatives_that_disagree_leave_a_binding_unknown() {
    let types = "enum Choice = A({ a: Nat }), B({ b: Nat });";
    assert_eq!(
        typed(
            types,
            "B({ b: 1 });",
            "@values.filter_match(|(A(x) | B(x))| x.b)",
            Some("Choice")
        )
        .unwrap(),
        "1;"
    );
}

#[test]
fn typed_update_checks_the_parts_of_the_new_value() {
    let types = "enum P = P { a: Nat };\nenum C = C { xs: [Nat], p: P };";
    let input = "C { xs: [1], p: P { a: 1 } };";
    // At the value, without --type.
    let e = typed_err(types, input, r#"@values.map(|c| c.update(|c| c.xs = ["bad"]))"#);
    assert!(e.contains("does not fit") && e.contains("of type [Nat]"), "{}", e);
    let e = typed_err(types, input, r#"@values.map(|c| c.update(|c| c.p = P { a: "x" }))"#);
    assert!(e.contains("does not fit") && e.contains("of type P"), "{}", e);
    assert_eq!(
        typed_ok(
            types,
            input,
            "@values.map(|c| c.update(|c| c.xs = [2, 3]).update(|c| c.p = P { a: 4 }))"
        ),
        "C { xs: [2, 3], p: P { a: 4 } };"
    );
    // Before the query runs, with --type.
    let e = typed(
        types,
        input,
        r#"@values.map(|c| c.update(|c| c.xs = ["bad"]))"#,
        Some("C"),
    )
    .unwrap_err();
    assert!(
        e.starts_with("before any value") && e.contains("of type [Nat]"),
        "{}",
        e
    );
    let e = typed(
        types,
        input,
        r#"@values.map(|c| c.update(|c| c.p = P { b: 1 }))"#,
        Some("C"),
    )
    .unwrap_err();
    assert!(e.starts_with("before any value"), "{}", e);
}
