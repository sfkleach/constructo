# Plan: step 8 of codegen, publishing the runtime

This plans step 8 of the [implementation plan](implementation-plan.md): making `constructo-runtime` fit to be
published, and the release able to publish it with `constructo`. The release itself is cut from `main` after the merge,
so the step is done when a release would publish both.

## What there is today

- **The crate** `constructo-runtime` 0.1.0 has a `description`, `license = "MIT"` and `repository`, and no README,
  `readme`, `keywords`, `categories` or `LICENSE` file. `constructo` depends on it with
  `version = "=0.1.0", path = "constructo-runtime"`.
- **Its name is free on crates.io,** and so is `constructo`, which has never been published either. *Checked* with the
  crates.io API, which answers 404 for both.
- **`cargo publish --workspace` publishes both, the runtime first.** Cargo 1.98 orders a workspace by its dependencies,
  and verifies `constructo` against a local registry that has the runtime in it. *Checked* with
  `cargo publish --workspace --dry-run`, which verified and would have uploaded both, in that order.
- **The package of the runtime has `constructo-runtime/tests/conformance.rs`,** which reads `syntax/conformance`, so
  it fails in the published crate. Nothing runs it there, but it is a file that cannot work.
- **The public API.** 87 public items have no doc comment: 52 in `ast`, 10 in `diagnostics`, 19 in `lexer` and 6 in
  `parser`, counted with `-W missing_docs`. Most are the fields and variants of `ast`. `lexer::scan_string` is public
  for the lexer of queries in `constructo`, and `values_text::bare` for the tests of `constructo`.
- **The release recipes,** `draft-release` and `publish-release` in the `Justfile`, push a signed tag that triggers
  `release-draft.yml`, and run `release-publish.yml`. **Neither workflow is in the repository,** in any branch or in the
  history: `.github/` holds only `copilot-instructions.md`. The outline thought that only the second was missing. So a
  release today pushes a tag that triggers nothing. `.local/scripts/extract-changelog.py` gives the notes of a release
  from the CHANGELOG, for the draft, and `check-changelog.py`, which `shippable` runs, checks the version of
  `constructo` in `Cargo.toml`, `Cargo.lock` and the CHANGELOG, and not that of the runtime.
- **The package of `constructo`** has every tracked file: `src/`, `syntax/`, `docs/`, and the tooling, `.claude/`,
  `.local/`, `Justfile` and `tests/functest.sh`. Nothing private, since only tracked files are packaged, but more than a
  user of the crate needs.

## The public API

- **`ast` stays public as it is,** and is the tree of a value that decision 47 says every runtime makes public. Its
  enums are not marked `#[non_exhaustive]`: that would make `constructo`, which is another crate, and the generated
  code match with a wildcard arm, and lose the compiler's check that every kind of value is handled. Before 1.0, a
  change to it is a change of the minor version, as Cargo's rules allow, and the generated code names the minor version
  that it needs. **Recommended.**
- **`lexer::scan_string` and `values_text::bare` are hidden from the documentation,** with `#[doc(hidden)]` and a
  comment that says who uses them, and are not a promise. Renaming them gains nothing, and making them private needs a
  copy in `constructo`. **Recommended.**
- **Every public item gets a doc comment,** and the crate gets `#![warn(missing_docs)]`, so that clippy with
  `-D warnings` keeps it so.
- **The types parser stays in the runtime,** as decision 46 says. Its items are documented as the parser of
  `constructo check`, which generated code does not use.

## The crate on its own

- **A README,** `constructo-runtime/README.md`, named by `readme`: what the crate is, that generated code depends on it
  and that a program adds it at the version that the generated file's header names, the tree of a value with `parse`
  and `write_text`, and what the version means before 1.0.
- **A copy of `LICENSE`,** since the MIT licence asks that the notice go with the code, and Cargo packages only the
  files of the crate's folder. A test checks that the copy is the same as the root's, so that they cannot drift.
- **`keywords` and `categories`,** as `parser-implementations` and `encoding`, and `exclude = ["tests/"]`, so that the
  conformance test, which needs the repository, is not published.
- **`cargo doc -p constructo-runtime`** builds with no warning, and `cargo package` lists only the README, the licence,
  the manifest and `src/`.

## The release

- **`check-changelog.py`** also checks that the runtime has the version of `constructo`, and that the dependency is
  `=` that version, so that `shippable`, which `draft-release` runs, refuses a release where they differ.
- **`release-draft.yml`,** on a pushed tag `v*`: checks out the tag, runs the tests as `just test` does, checks
  `shippable`, runs `cargo publish --workspace --dry-run`, and makes a draft release on GitHub with the notes of
  `extract-changelog.py`. A tag with a suffix, as `v0.2.0-rc1`, makes a pre-release.
- **`release-publish.yml`,** run by hand with the tag, as `publish-release` runs it: refuses a tag with a suffix, checks
  out the tag, runs `cargo publish --workspace`, which publishes the runtime and then `constructo`, and then makes the
  draft release published. It has no API token of its own: it gets one from crates.io by trusted publishing.
- **The comment in the `Justfile`** that says the recipes do not yet publish the runtime is replaced by one that says
  how they do.

## Decided with the user

1. **How the first release is published.** **Decided: the first release by hand.** crates.io gives trusted
   publishing only to a crate that exists, and is set up on the crate's page, so neither crate can be published first
   by a workflow without a token. Two ways:
   1. **The first release by hand:** `cargo publish --workspace` from the tag, with a token of the user's, after which
      trusted publishing is set up for both crates, and every later release is by the workflow. The workflow and the
      `Justfile` say so. **Chosen**, since no token is stored anywhere.
   2. **A token as a secret of the repository,** used by the workflow for the first release, and removed when trusted
      publishing is set up.
2. **Whether the workflows are written here or come from another project of the user's.** **Decided: written here.**
   The recipes were copied from another project, whose workflows did not come with them. What matters is their intent:
   a signed tag makes a draft release that has been tested, and publishing is a separate step, run by hand, that
   publishes only a stable tag and then the draft. The workflows are written to that, with the actions pinned by their
   commit.
3. **What the package of `constructo` holds.** **Decided: the `exclude`, here,** since this is the step that makes the
   first release possible. An `exclude` of `.claude/`, `.local/`, `Justfile` and `tests/` publishes what a user of the
   crate needs, `src/`, `syntax/`, `docs/`, the README, the licence and the CHANGELOG, and not the tooling.

## How it is checked

- `cargo doc -p constructo-runtime` with `RUSTDOCFLAGS="-D warnings"`, and clippy with `missing_docs`.
- `cargo package -p constructo-runtime --list`, and `cargo publish --workspace --dry-run`.
- The workflows with `actionlint`, if it is installed, and otherwise read. They cannot run until they are on GitHub.
- `check-changelog.py` against a runtime with another version, which it refuses.
- `just test`.

## The commits

1. The public API: the doc comments, `#![warn(missing_docs)]`, and the two items hidden.
2. The crate on its own: the README, the licence and its test, and the manifest.
3. The release: `check-changelog.py`, the two workflows, the `Justfile`, and the `exclude` of `constructo`.
4. The draft and the outline brought up to date. The move of the task to `docs/tasks/done/` comes after the review, as
   the last commit before the merge.

## As it was built

*Done,* in the first three commits above. What the plan did not say:

- **`constructo-runtime/tests/conformance.rs` already skips itself** outside the repository, so it did not fail in the
  published crate, as the plan said. The `exclude` of `tests/` keeps it, and the test of the licence, which reads the
  root's `LICENSE`, out of the package all the same, and `cargo package` warns that it is leaving them out.
- **The doc comment of `Severity` said that only `convert` gives warnings,** which `codegen` made untrue, and the
  sentence is gone. The doc of the module `parser` says that it reads values files as well as types files.
- **The README's example of the tree was compiled and run** in a crate of its own, outside the repository.
- **The `exclude` of `constructo` is wider than the plan's list:** it also has `.cargo/`, `.github/`, `CLAUDE.md` and
  `.coderabbit.yaml`, which are tooling too. It does not name `constructo-runtime/`, which Cargo leaves out of the
  package of `constructo` since it is a package of its own.
- **`release-publish.yml` refuses a tag that has no draft release,** since the draft is the record that
  `release-draft.yml` tested the tag, and `release-draft.yml` checks the tag against the version of `Cargo.toml`, as
  the recipe does, so that a tag pushed by hand is held to it too. Each passes the tag to its scripts through the
  environment, and not written into them, so that the text of a tag cannot be run.
- **The actions are pinned** to `actions/checkout` 7.0.1, `taiki-e/install-action` 2.90.11 and
  `rust-lang/crates-io-auth-action` 1.0.6, by their commits.
- **The workflows have not run,** since they run only on GitHub, and `actionlint` is not installed here: they were
  read, and parsed as YAML. The first tag pushed after the merge is their first test.
- **`check-changelog.py` was tried** on copies of the manifests and the lock with the CHANGELOG released, and refuses a
  runtime of another version, in its manifest or in the lock, and a dependency that is not `=` the version.

## Done when

- `cargo publish --workspace --dry-run` publishes both, and the runtime's package has only what it needs.
- Every public item of the runtime is documented, and the documentation builds with no warning.
- A release by the recipes would publish both, in order, as the workflows and the `Justfile` say.
- `just test` passes.
