---
name: review-specs
description: >-
  Review the specifications in docs/specs/ against the code, the tests, the examples and the decisions, correct what
  is wrong in them, and then suggest new spec files for behaviour that no spec describes. Use when the user runs
  /review-specs, optionally naming spec files to limit the review to.
argument-hint: "[spec-file ...]"
disable-model-invocation: true
---

# Review the specs

The files in `docs/specs/` say what `constructo` does, precisely enough that someone could rebuild it or rely on it.
This review checks that every claim in them is still true, corrects the ones that are not, and ends with suggestions
for specs that are missing.

Scope: the files named in `$ARGUMENTS`, or every `docs/specs/*.md` if none are named. The suggestions for new specs are
made in every case.

## Ground rules

- **The code is the record of what is done; the decisions are the record of what was meant.** A spec that disagrees
  with code that agrees with its decision and its tests is wrong, and is corrected. Code that disagrees with a decision,
  or with a spec that the tests or examples back up, is a finding for the user. Do not change code, tests or examples
  in this review, and do not "correct" a spec to match code that looks like a bug.
- **Every correction has evidence**: a line of code, a test, an example, or the output of a run of the binary. A claim
  that cannot be checked is reported, and is not rewritten from memory.
- **Check by running, not only by reading**, where the claim is about behaviour: messages, exit codes, defaults,
  option names, the order of things. Build with `cargo build`. The binary is `_build/target/debug/constructo`. Make any
  files for experiments in the scratchpad folder, or under `_build/`, and never elsewhere in the repository.
- **Keep the voice of the specs.** They are written in plain words and short sentences, they say what happens rather
  than what is hoped for, and they keep to the house style in `CLAUDE.md`: lines of at most 120 characters, LF endings,
  a newline at the end, no trailing whitespace except the single space of a Markdown line break. Make the smallest edit
  that makes a passage true. Do not restructure a spec unless it cannot be corrected otherwise.
- **Do not commit.** Leave the corrections as changes in the working tree, and say what they are. On `main` this is what
  `CLAUDE.md` requires; on a branch the user decides.

## Steps

### 1. Mechanical checks

Run, from the root of the repository:

```sh
python3 -I .claude/skills/review-specs/check_specs.py [FILES]
```

It reports broken relative links and anchors, paths in backticks that do not exist, decisions named by a number that has
no file, and breaches of the house style. Fix what it reports, and run it again at the end, when it must report nothing.

### 2. Find the sources of truth for each spec

For each spec, find the code, tests and examples that implement what it describes, by searching for the names, keys,
options and messages that it mentions. As a starting point (check it; files move):

- `config-file.md`: `src/config.rs` and the options in `src/main.rs`; `syntax/examples/config/` and the output of
  `--print-config`.
- `project-file-format.md`: `src/project.rs`; `syntax/examples/project/` and the unit tests in the module.
- `lock-file-format.md`: `src/lock.rs`, `src/verify.rs` and `src/digest.rs`; their unit tests and the lock sections of
  `tests/functest.sh`.
- `resolving-identifiers.md`: `src/resolve.rs`; the `*names*` folders in `syntax/examples/project/`.
- `cache-management.md`: `Cache` and `Scratch` in `src/fetch.rs`; its unit tests and the repository functests.
- `git-management.md`: `git`, `child_environment`, `SETTINGS` and `validate_url` in `src/fetch.rs`; the same tests.

Read the decisions that the spec names, and any others in `docs/decisions/` on the same subject.

### 3. Check every claim

Go through the spec section by section. For each statement, find the evidence or run the binary. In particular:

- **Messages.** Every quoted message, whole or in part, is found in `src/` with `grep -rnF`, allowing for the
  placeholders that the spec writes as `X`, `FILE` and the like. Check the exit code that the spec gives for it.
- **Names.** Options, keys, tables, environment variables, file names and folders are spelt as the code spells them,
  and every one that the code has is in the spec, if the spec claims to list them.
- **Defaults, limits and orders.** Defaults, numbers, precedence ("the first of these that is set"), and the order in
  which things are done or reported match the code.
- **Formats.** Examples of files and of output are produced by the binary, or parse with it, as they are written.
  Worked examples, such as digests, are recomputed.
- **Absolutes.** "Never", "always", "only", "every" and "nothing else" are true without exceptions in the code. These
  are where specs most often go out of date.
- **Staleness.** Words such as "not yet", "planned", "for now" and "will" describe something that may since have been
  done or dropped. Statements of what is tested ("tested with git 2.43") are still true.
- **Consistency between specs.** A fact that two specs both state, such as an exit code, the location of a file or a
  rule about names, is stated the same way in both, and the "Related" sections link where they should.
- **Consistency with the rest of `docs/`.** The README, `docs/vision.md`, `CHANGELOG.md` and `syntax/grammar.md` do not
  contradict the spec. Report a contradiction there; correct it only if it is a plain mistake of fact.

### 4. Correct

Edit the spec where it is wrong. Keep a list of each correction: the file and line, what it said, what it says now,
and the evidence. Then run the mechanical checks again.

### 5. Look for specs that are missing

A spec is missing when there is behaviour that a user or a program can rely on, which is specified nowhere but in the
code, a decision, a task document, `--help` or the CHANGELOG. Look at:

- the subcommands and options in `--help`, and whether a spec describes each one's behaviour and output;
- the formats that the tool reads or writes, including its diagnostics, its verbose output and its exit codes;
- the language itself: what `syntax/grammar.md` leaves to the decisions, such as the typing rules and the checks after
  parsing;
- the public library API in `src/lib.rs`;
- the decisions in `docs/decisions/` that no spec names, which often mark behaviour that has no spec;
- the CHANGELOG entries that no spec covers.

For each candidate, say what the spec would cover, where the truth is today, who relies on it, and how much the lack of
it costs. Do not write the new specs: suggest them, ranked, and let the user choose.

## Report

End with a report in this order, and keep it short enough to read in one go:

1. **Corrections made**: for each, the file and line, the change, and the evidence. Say that they are not committed.
2. **Disagreements for the user**: where the code, a decision, a test or another document disagree and the spec was
   left alone, with what each says and a recommendation.
3. **Checked and found right**: a sentence per spec, so that the user knows what was covered and how (read, or run).
4. **Not checked**: anything that could not be verified here, such as behaviour on Windows, and why.
5. **Suggested new specs**: ranked, each with a proposed file name in `docs/specs/`, its scope, its sources of truth
   today, and why it is worth writing.
