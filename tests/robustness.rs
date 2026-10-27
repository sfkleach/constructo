//! Robustness tests: mutated versions of the examples must never make the library panic, overflow the stack or run for
//! a long time. A diagnostic is the right answer to bad input, and so is success.

use constructo::{check_sources_with, prelude_source, CheckOptions, Experimental, Source};
use proptest::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// A stack that is smaller than the usual 8 MB, so that recursion which is too deep shows up here. A debug build uses
/// about 6 KB for each level of nesting, which is 1.2 MB at the limit of 200 levels, so this leaves a margin of three.
const STACK_BYTES: usize = 4 * 1024 * 1024;
const TIME_LIMIT: Duration = Duration::from_secs(10);

/// Snippets that are likely to upset a parser or a checker when they are put in the wrong place.
const SNIPPETS: &[&str] = &[
    "[",
    "]",
    "{",
    "}",
    "(",
    ")",
    "<",
    ">",
    "?",
    "??",
    ",",
    ";",
    ":",
    "\"",
    "//",
    "-",
    "+1",
    "enum ",
    "const ",
    "Option<",
    "Some(",
    "None",
    "T",
    "_T",
    "[T]",
    "Option<[T]>",
    "x: ",
    "= ",
    "Nat",
    "Float",
    "é",
    "\n",
];

fn examples(dir: &str, extension: &str) -> Vec<String> {
    fn walk(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, extension, out);
            } else if path.extension().is_some_and(|e| e == extension) {
                out.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("syntax/examples").join(dir),
        extension,
        &mut paths,
    );
    paths.iter().filter_map(|p| std::fs::read_to_string(p).ok()).collect()
}

#[derive(Debug, Clone)]
enum Mutation {
    Delete { at: u16, len: u8 },
    Duplicate { at: u16, len: u8 },
    Insert { at: u16, snippet: u8 },
}

fn mutation() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        (any::<u16>(), 1u8..40).prop_map(|(at, len)| Mutation::Delete { at, len }),
        (any::<u16>(), 1u8..40).prop_map(|(at, len)| Mutation::Duplicate { at, len }),
        (any::<u16>(), any::<u8>()).prop_map(|(at, snippet)| Mutation::Insert { at, snippet }),
    ]
}

/// The character boundary that `at` selects, as a fraction of the length of the text.
fn boundary(text: &str, at: u16) -> usize {
    let mut index = text.len() * at as usize / (u16::MAX as usize + 1);
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn mutate(text: &str, mutations: &[Mutation]) -> String {
    let mut text = text.to_string();
    for mutation in mutations {
        match mutation {
            Mutation::Delete { at, len } => {
                let start = boundary(&text, *at);
                let end = text[start..]
                    .char_indices()
                    .nth(*len as usize)
                    .map_or(text.len(), |(i, _)| start + i);
                text.replace_range(start..end, "");
            }
            Mutation::Duplicate { at, len } => {
                let start = boundary(&text, *at);
                let end = text[start..]
                    .char_indices()
                    .nth(*len as usize)
                    .map_or(text.len(), |(i, _)| start + i);
                let copy = text[start..end].to_string();
                text.insert_str(end, &copy);
            }
            Mutation::Insert { at, snippet } => {
                let start = boundary(&text, *at);
                text.insert_str(start, SNIPPETS[*snippet as usize % SNIPPETS.len()]);
            }
        }
    }
    text
}

/// Runs the check on a thread with a small stack, and fails if it panics or takes too long.
fn check(sources: Vec<Source>, options: CheckOptions) -> Result<(), String> {
    let (sender, receiver) = mpsc::channel();
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(move || {
            let report = check_sources_with(&sources, &options);
            let _ = sender.send(report.is_ok());
        })
        .expect("a thread can be started");
    match receiver.recv_timeout(TIME_LIMIT) {
        Ok(_) => Ok(()),
        Err(mpsc::RecvTimeoutError::Timeout) => Err("took more than the time limit".to_string()),
        Err(mpsc::RecvTimeoutError::Disconnected) => match handle.join() {
            Err(panic) => Err(format!(
                "panicked: {}",
                panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default()
            )),
            Ok(()) => Err("the thread ended without a result".to_string()),
        },
    }
}

fn options(bits: u8) -> CheckOptions {
    CheckOptions {
        type_check: bits & 1 == 0,
        widening: bits & 2 == 0,
        continue_after_error: bits & 4 != 0,
        allow_refs: bits & 8 != 0,
        experimental: if bits & 16 != 0 {
            Experimental::all()
        } else {
            Experimental::default()
        },
        ..CheckOptions::default()
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1000, ..ProptestConfig::default() })]

    #[test]
    fn mutated_examples_never_crash(
        types in any::<prop::sample::Index>(),
        values in any::<prop::sample::Index>(),
        types_mutations in prop::collection::vec(mutation(), 0..6),
        values_mutations in prop::collection::vec(mutation(), 0..6),
        bits in any::<u8>(),
    ) {
        let types_files = examples("types", "types");
        let values_files = examples("values", "values");
        let sources = vec![
            prelude_source(),
            Source {
                name: "a.types".to_string(),
                text: mutate(&types_files[types.index(types_files.len())], &types_mutations),
            },
            Source {
                name: "b.values".to_string(),
                text: mutate(&values_files[values.index(values_files.len())], &values_mutations),
            },
        ];
        if let Err(problem) = check(sources, options(bits)) {
            prop_assert!(false, "{}", problem);
        }
    }
}

// Generated text, which gets much further into the checker than a mutated example does.

const TYPE_NAMES: &[&str] = &[
    "Nat", "Int", "Float", "String", "Bool", "Option", "A", "B", "C", "T", "U",
];
const VALUE_NAMES: &[&str] = &["A", "B", "C", "Some", "None", "True", "x", "null", "a1", "a2"];
const FIELD_NAMES: &[&str] = &["x", "y", "name"];

fn type_text() -> impl Strategy<Value = String> {
    let leaf = prop::sample::select(TYPE_NAMES).prop_map(|s| s.to_string());
    leaf.prop_recursive(6, 40, 4, |inner| {
        prop_oneof![
            inner.clone().prop_map(|t| format!("[{}]", t)),
            inner.clone().prop_map(|t| format!("{}?", t)),
            (
                prop::sample::select(TYPE_NAMES),
                prop::collection::vec(inner.clone(), 1..3)
            )
                .prop_map(|(n, args)| format!("{}<{}>", n, args.join(", "))),
            prop::collection::vec((prop::sample::select(FIELD_NAMES), inner), 0..3).prop_map(|fields| {
                let fields: Vec<String> = fields.iter().map(|(n, t)| format!("{}: {}", n, t)).collect();
                format!("{{ {} }}", fields.join(", "))
            }),
        ]
    })
}

fn variant_text() -> impl Strategy<Value = String> {
    (
        prop::sample::select(&["A", "B", "C", "D"][..]),
        type_text(),
        type_text(),
        0u8..6,
    )
        .prop_map(|(name, t1, t2, shape)| match shape {
            0 => name.to_string(),
            1 => format!("{}[{}]", name, t1),
            2 => format!("{}({})", name, t1),
            3 => format!("{}({}, n: {})", name, t1, t2),
            4 => format!("{} {{ x: {}, y: {} }}", name, t1, t2),
            _ => format!("{}()", name),
        })
}

fn enum_text() -> impl Strategy<Value = String> {
    (
        prop::sample::select(&["A", "B", "C", "E"][..]),
        prop::collection::vec(prop::sample::select(&["T", "U"][..]), 0..3),
        prop::collection::vec(variant_text(), 1..4),
    )
        .prop_map(|(name, params, variants)| {
            let params = if params.is_empty() {
                String::new()
            } else {
                format!("<{}>", params.join(", "))
            };
            format!("enum {}{} = {};", name, params, variants.join(", "))
        })
}

fn value_text() -> impl Strategy<Value = String> {
    let leaf = prop_oneof![
        Just("1".to_string()),
        Just("-1".to_string()),
        Just("2.5".to_string()),
        Just("\"s\"".to_string()),
        prop::sample::select(VALUE_NAMES).prop_map(|s| s.to_string()),
    ];
    leaf.prop_recursive(6, 40, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..3).prop_map(|v| format!("[{}]", v.join(", "))),
            inner.clone().prop_map(|v| format!("{}?", v)),
            (
                prop::sample::select(VALUE_NAMES),
                prop::collection::vec(inner.clone(), 0..3)
            )
                .prop_map(|(n, v)| format!("{}({})", n, v.join(", "))),
            (
                prop::sample::select(VALUE_NAMES),
                prop::collection::vec(inner.clone(), 0..3)
            )
                .prop_map(|(n, v)| format!("{}[{}]", n, v.join(", "))),
            (prop::sample::select(VALUE_NAMES), inner.clone(), inner)
                .prop_map(|(n, a, b)| format!("{} {{ x: {}, y: {} }}", n, a, b)),
        ]
    })
}

fn const_text() -> impl Strategy<Value = String> {
    (
        prop::sample::select(&["a1", "a2", "x"][..]),
        proptest::option::of(type_text()),
        value_text(),
        any::<bool>(),
    )
        .prop_map(|(name, ty, value, generic)| {
            let params = if generic { "<T>" } else { "" };
            match ty {
                Some(ty) => format!("const {}{}: {} = {};", name, params, ty, value),
                None => format!("const {} = {};", name, value),
            }
        })
}

fn types_text() -> impl Strategy<Value = String> {
    (
        prop::collection::vec(enum_text(), 0..4),
        prop::collection::vec(const_text(), 0..4),
    )
        .prop_map(|(enums, consts)| format!("{}\n{}\n", enums.join("\n"), consts.join("\n")))
}

fn values_text() -> impl Strategy<Value = String> {
    prop::collection::vec(value_text(), 0..4).prop_map(|v| v.join(";\n"))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1000, ..ProptestConfig::default() })]

    #[test]
    fn generated_text_never_crashes(types in types_text(), values in values_text(), bits in any::<u8>()) {
        let sources = vec![
            prelude_source(),
            Source { name: "a.types".to_string(), text: types },
            Source { name: "b.values".to_string(), text: values },
        ];
        if let Err(problem) = check(sources, options(bits)) {
            prop_assert!(false, "{}", problem);
        }
    }
}

/// Text that is nested to a depth chosen by the test, in each of the places that nest.
fn nested(kind: u8, depth: usize) -> String {
    let open = |s: &str| s.repeat(depth);
    match kind {
        0 => format!("const a = {}1{};", open("["), "]".repeat(depth)),
        1 => format!("enum E = A({}Nat{});", open("["), "]".repeat(depth)),
        2 => format!("enum E = A(Nat{});", open("?")),
        3 => format!("const a = {}1{};", open("A("), ")".repeat(depth)),
        4 => format!("const a = {}1{};", open("A["), "]".repeat(depth)),
        5 => format!("enum E = A({}Nat{});", open("Option<"), ">".repeat(depth)),
        6 => format!("enum E = A({}Nat{});", open("{ x: "), " }".repeat(depth)),
        _ => format!("const a = 1{};", open("?")),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn deep_nesting_never_crashes(kind in 0u8..8, depth in 1usize..3000, bits in any::<u8>()) {
        let sources = vec![
            prelude_source(),
            Source { name: "a.types".to_string(), text: nested(kind, depth) },
        ];
        if let Err(problem) = check(sources, options(bits)) {
            prop_assert!(false, "kind {} depth {}: {}", kind, depth, problem);
        }
    }
}

// Metamorphic properties: the outcome must change in the right way when the input or an option is changed.

fn outcome(types: &str, values: &str, options: &CheckOptions) -> bool {
    let sources = vec![
        prelude_source(),
        Source {
            name: "a.types".to_string(),
            text: types.to_string(),
        },
        Source {
            name: "b.values".to_string(),
            text: values.to_string(),
        },
    ];
    check_sources_with(&sources, options).is_ok()
}

fn declarations_reversed(types: &str) -> String {
    let mut lines: Vec<&str> = types.lines().collect();
    lines.reverse();
    lines.join("\n")
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1000, ..ProptestConfig::default() })]

    /// Decision 1: the order of declarations does not matter. Each generated declaration is on one line.
    #[test]
    fn the_order_of_declarations_does_not_matter(types in types_text(), values in values_text(), bits in any::<u8>()) {
        let options = options(bits);
        prop_assert_eq!(
            outcome(&types, &values, &options),
            outcome(&declarations_reversed(&types), &values, &options)
        );
    }

    #[test]
    fn widening_only_accepts_more(types in types_text(), values in values_text(), bits in any::<u8>()) {
        let strict = CheckOptions { widening: false, ..options(bits) };
        let loose = CheckOptions { widening: true, ..options(bits) };
        prop_assert!(!outcome(&types, &values, &strict) || outcome(&types, &values, &loose));
    }

    #[test]
    fn skipping_the_type_check_only_accepts_more(types in types_text(), values in values_text(), bits in any::<u8>()) {
        let checked = CheckOptions { type_check: true, ..options(bits) };
        let unchecked = CheckOptions { type_check: false, ..options(bits) };
        prop_assert!(!outcome(&types, &values, &checked) || outcome(&types, &values, &unchecked));
    }

    #[test]
    fn switching_on_the_experiments_only_accepts_more(
        types in types_text(),
        values in values_text(),
        bits in any::<u8>(),
    ) {
        let off = CheckOptions { experimental: Experimental::default(), ..options(bits) };
        let on = CheckOptions { experimental: Experimental::all(), ..options(bits) };
        prop_assert!(!outcome(&types, &values, &off) || outcome(&types, &values, &on));
    }

    #[test]
    fn allowing_references_only_accepts_more(types in types_text(), values in values_text(), bits in any::<u8>()) {
        let off = CheckOptions { allow_refs: false, ..options(bits) };
        let on = CheckOptions { allow_refs: true, ..options(bits) };
        prop_assert!(!outcome(&types, &values, &off) || outcome(&types, &values, &on));
    }

    #[test]
    fn larger_limits_only_accept_more(types in types_text(), values in values_text(), bits in any::<u8>()) {
        let small = CheckOptions { infer_limit: 20, default_limit: 20, ..options(bits) };
        let large = CheckOptions { infer_limit: 10_000, default_limit: 10_000, ..options(bits) };
        prop_assert!(!outcome(&types, &values, &small) || outcome(&types, &values, &large));
    }
}

#[test]
fn the_generated_text_is_not_always_rejected() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;
    let mut runner = TestRunner::deterministic();
    let strategy = (types_text(), values_text());
    let (mut ok, mut types_ok, total) = (0, 0, 2000);
    for _ in 0..total {
        let (types, values) = strategy.new_tree(&mut runner).unwrap().current();
        let options = CheckOptions::default();
        ok += usize::from(outcome(&types, &values, &options));
        types_ok += usize::from(outcome(&types, "", &options));
    }
    // Most of the text is rejected, but if none of it is accepted then the properties above are not testing anything.
    assert!(
        ok > 0 && types_ok > 0,
        "accepted {} of {} and {} of {} types alone",
        ok,
        total,
        types_ok,
        total
    );
}
