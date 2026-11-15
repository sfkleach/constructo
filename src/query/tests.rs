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
