//! The conformance suite of `syntax/conformance`, run against this runtime, which is the one that the others are held
//! to. See `syntax/conformance/README.md` for what a case is.
//!
//! With `CONSTRUCTO_BLESS=1` in the environment, the expected results are written from what this runtime gives,
//! instead of being compared with it. That is how they are made, and how they are changed when the syntax changes.
//!
//! The suite is in the repository and not in the published crate, so a copy of the crate that is not in the repository,
//! such as one downloaded from crates.io, skips these tests.

use constructo_runtime::diagnostics::Diagnostic;
use constructo_runtime::parser::{parse_values_with, Experimental};
use constructo_runtime::values_text::render;
use std::fs;
use std::path::{Path, PathBuf};

/// The root of the repository, or `None` when the crate is not in one. The repository is recognised by the
/// `Cargo.toml` of its workspace, beside this crate's folder. Within the repository the suite must be found, so that a
/// suite that has been moved or lost is a failure and not a skip.
fn repository() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let manifest = fs::read_to_string(root.join("Cargo.toml")).ok()?;
    if !manifest.contains("\"constructo-runtime\"") {
        return None;
    }
    assert!(
        root.join("syntax/conformance").is_dir(),
        "the repository has no syntax/conformance"
    );
    Some(root)
}

/// The repository, or `None` after saying that the tests are skipped.
fn repository_or_skip() -> Option<PathBuf> {
    let root = repository();
    if root.is_none() {
        eprintln!("The conformance suite is skipped, since this crate is not in the Constructo repository.");
    }
    root
}

/// Every file under the folder whose name has the extension, sorted, so that failures are reported in a stable order.
fn files(folder: &Path, extension: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut folders = vec![folder.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == extension) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// A case is read with the experimental features on when a folder on its path is called `experimental`.
fn experimental(relative: &Path) -> Experimental {
    if relative.components().any(|c| c.as_os_str() == "experimental") {
        Experimental::all()
    } else {
        Experimental::default()
    }
}

/// What the runtime gives for a case: the values written back, each on a line of its own, or the line of the error.
fn result(path: &Path, relative: &Path) -> String {
    let text = fs::read_to_string(path).unwrap();
    match parse_values_with(&text, experimental(relative)) {
        Ok(values) => values.iter().map(|value| render(value, true)).collect(),
        Err(error) => {
            let name = path.file_name().unwrap().to_string_lossy();
            let diagnostic = Diagnostic::new(&name, error.pos, error.message);
            let diagnostic = match error.context {
                Some(context) => diagnostic.with_note(format!("in {}", context)),
                None => diagnostic,
            };
            format!("{}\n", diagnostic)
        }
    }
}

#[test]
fn every_case_gives_its_expected_result() {
    let Some(repository) = repository_or_skip() else {
        return;
    };
    let bless = std::env::var_os("CONSTRUCTO_BLESS").is_some();
    let root = repository.join("syntax/conformance");
    let cases = files(&root, "values");
    assert!(cases.len() > 50, "the suite has only {} cases", cases.len());
    let mut failures = Vec::new();
    for case in &cases {
        let relative = case.strip_prefix(&root).unwrap();
        let actual = result(case, relative);
        let expected_path = case.with_extension("expected");
        if bless {
            fs::write(&expected_path, &actual).unwrap();
            continue;
        }
        match fs::read_to_string(&expected_path) {
            Ok(expected) if expected == actual => {}
            Ok(expected) => failures.push(format!(
                "{}:\n--- expected\n{}--- found\n{}",
                relative.display(),
                expected,
                actual
            )),
            Err(_) => failures.push(format!("{}: has no .expected file", relative.display())),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_expected_result_has_a_case() {
    let Some(repository) = repository_or_skip() else {
        return;
    };
    let root = repository.join("syntax/conformance");
    let orphans: Vec<_> = files(&root, "expected")
        .into_iter()
        .filter(|path| !path.with_extension("values").exists())
        .map(|path| path.strip_prefix(&root).unwrap().display().to_string())
        .collect();
    assert!(orphans.is_empty(), "expected results with no case: {:?}", orphans);
}

/// The examples of values are copied into the suite, so that the suite is whole in itself. This keeps the copies the
/// same as the examples, so that the two cannot drift apart.
#[test]
fn every_example_of_values_is_a_case() {
    let Some(repository) = repository_or_skip() else {
        return;
    };
    let examples = repository.join("syntax/examples/values");
    let copies = repository.join("syntax/conformance/examples");
    let mut failures = Vec::new();
    for example in files(&examples, "values") {
        let relative = example.strip_prefix(&examples).unwrap();
        match fs::read(copies.join(relative)) {
            Ok(copy) if copy == fs::read(&example).unwrap() => {}
            Ok(_) => failures.push(format!("{} differs from its example", relative.display())),
            Err(_) => failures.push(format!("{} has no copy in the suite", relative.display())),
        }
    }
    for copy in files(&copies, "values") {
        let relative = copy.strip_prefix(&copies).unwrap();
        if !examples.join(relative).exists() {
            failures.push(format!("{} is not an example any more", relative.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
