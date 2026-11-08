# Plan: hidden and renamed names in imports

This plans [hide-and-replace.md](hide-and-replace.md). It builds on decision 32, which forbids two projects of one
group to declare one name. Hiding and renaming are the way out of such a clash that does not need the groups to be
changed, and they work from the importing project, whose project-file the user owns.

## What the code does today

- `ImportTable` in `src/project.rs` is the serde form of an `[[import]]`. It is strict (`deny_unknown_fields`), and
  `Import` is the loaded form. Both hold the group and the place, and nothing about names.
- `main.rs::layout` and `lib.rs::plain_layout` turn the imports into `ProjectInfo.imports: Vec<(Option<String>,
  usize)>`, the group and the index of the target. `resolve.rs` is the only user of that.
- Names are resolved in `Resolver::name`, which asks `declares(home, ns, name)` of each project of a group. The
  canonical name is `Name@project` and it is what the later checks see, so the validity and type checks need no change.
- Decision 32 is enforced by `Resolver::clashes`, which compares the names that each project of a group declares.
- `Resolution.visible_types` is the set of type names that a source can write bare. `validate.rs` uses it for the error
  `Type parameter X clashes with a declared type`.
- The lock file digests the files of a project and not how it is imported, so it needs no change.

## Design

Hide and rename change **what a project sees of an import**, and nothing about the imported project. Its own files see
its own names, and the canonical name of a declaration stays `Name@project`.

### The project-file

```toml
[[import]]
project-name = "com.example.base"
group = "base"
path = "../base"
hide = ["Debug", "LegacyPoint"]

[import.rename]
# The name used here = the name in the imported project.
Point = "BasePoint"
```

- `hide` is an array of names. `rename` is a table in which each key is the name that is written in the importing
  project and each value is the name that the imported project declares: `NAME_HERE = "NAME_THERE"`. So the example
  makes `BasePoint` of `com.example.base` be written `Point`. A key is unique in TOML, so two names cannot be renamed to
  one.
- Both are optional and default to nothing. The word is `hide` throughout.

### Rules

1. A name is a bare identifier. It applies to every declaration of that name in the project: the type, and the
   constructor or const, in whichever namespaces have one. This is blunt but is what decision 4 allows, since one
   spelling is written once in the project-file.
2. A name in `hide` or in the value of `rename` must be declared by the imported project. Otherwise it is an error at
   the name, in the project-file, so that a typo or a name that the project dropped is not silently ignored. The
   project-file is strict, and a bad import is exit code 2.
3. A name in `hide` and in a `rename` value is an error: the name cannot be both. A name is hidden, or renamed, or
   neither. A name is also not hidden or renamed twice.
4. A key of `rename` must be written like the name it replaces: a type or constructor name starts with an upper-case
   letter, and a const with a lower-case letter or `_`. It may not be a basic type name (`Nat`, `Int`, `Float`,
   `String`). `check_group`-style checks give the messages.
5. After an import is filtered, the **visible names** of the project in that import are its declared names without the
   hidden ones, with each renamed one under its new name. The old name of a renamed declaration is not visible.
6. Name lookup through a group, bare or `group::N`, sees the visible names. A qualification by full name,
   `a.b.c::N`, sees the declared names of the project: it names the project and not the import, and it is the way to
   get at a name that was hidden or renamed.
7. Decision 32 is applied to the visible names of each group of each importing project. A name that is hidden in the
   import where it would clash causes no error there. A project that imports the same pair without hiding it does get
   the error.
8. The project itself is not filtered, so its own declarations are always visible in its own `local`.
9. Messages show a name as it is written in the importing file. That already holds for the group and the bare form.
   With a rename the visible name must be shown and not the declared one, in `Names::show`.

### Where the clash is reported

Decision 32 reports a clash at the later declaration, which may be in an imported project that the user cannot edit.
Now that the importer can hide or rename, the message should say so. The plan keeps the position (it is the one that
every duplicate has, and it is a source file with a line and column) and adds a note: `first declared at ...; hide or
rename one in the import`. The owner has chosen to keep the position.

## Code changes

1. `project.rs`
   - `ImportTable`: `hide: Option<Vec<Spanned<String>>>` and
     `rename: Option<BTreeMap<Spanned<String>, Spanned<String>>>`.
   - `Import`: `hide: Vec<(String, Pos)>` and `rename: Vec<(String, String, Pos)>` (new name, declared name, place).
     The place of an entry is that of the `Spanned`.
   - Syntax checks at load: name shape (rule 4), hidden-and-renamed (rule 3), duplicates.
   - The check that the names exist (rule 2) can only be made when the target is loaded. `Loader::visit` already has the
     target's declarations only after its files are parsed, which happens later, in `lib.rs`. So the check is made in
     the resolution, as an error that is reported against the project-file. This needs a way to give a diagnostic a
     file that is not a source. The owner accepts a project-file error, exit code 2.
2. `resolve.rs`
   - Replace the tuple in `ProjectInfo.imports` with `ImportInfo { group, target, hide: HashSet<String>, rename:
     HashMap<String, String> }`, the rename keyed by the visible name. Fix `lib.rs::plain_layout`, `main.rs::layout`
     and the test helpers.
   - A method `visible(&self, import: &ImportInfo, ns, name) -> Option<String>` that gives the declared name that a
     written name stands for, or `None`. `group_homes` returns the imports and not only the homes, with the project
     itself as an unfiltered entry. `Resolver::name` uses it, and `declares` is used only for the unfiltered cases.
   - `Resolver::clashes` compares the visible names of the entries of a group. It needs the declarations of a home with
     their visible names, so it iterates the declarations of each import and maps each through the filter.
   - `visible_types(home)` returns the visible names.
   - `Names::rewrite` maps a canonical name back to the name written in the importing file, through the rename.
3. `lib.rs` and `main.rs`: build the new `ImportInfo`.

## Steps

### Step 1: hide

1. Add `hide` to `ImportTable` and `Import`, with the syntax checks and their unit tests.
2. Introduce `ImportInfo` and the visible-name lookup in `resolve.rs`, with hide only. No behaviour changes for an
   import with an empty filter, and the existing 282 unit tests and the functests must still pass.
3. Apply hide to the lookup, to `clashes` and to `visible_types`. Unit tests in `resolve.rs`: a hidden name is unknown
   where it was visible; a clash between two imports of one group goes away if one hides it; the hidden name is still
   reachable by the full name; the hidden name still works inside the imported project; a project that imports the
   same pair without hiding it is still an error; two imports of one project into two groups can hide differently.
4. Examples in `syntax/examples/project/`: `valid-names/hide` (two imports into `local` that share a name, one hiding
   it) and `invalid-names/hide-unknown`, `hide-twice` and so on, each with `// expect:`. The functest loops pick them
   up.
5. Spec: `project-file-format.md` (the table of `[[import]]` keys, a section on `hide`) and
   `resolving-identifiers.md` (visible names in the algorithm). Decision 33.

### Step 2: rename

1. Add `rename` to `ImportTable` and `Import` with its checks.
2. Extend `ImportInfo` and `visible` with the rename. The old name is hidden, the new one points at the old.
3. Rename in `Names::rewrite`, so that messages show the new name.
4. Unit tests: a renamed name resolves; the old name does not; a rename that causes a clash is reported (rule 7); a
   rename onto a name the same project also declares; a rename of a type does not touch a constructor of another
   spelling; a message shows the new name. Examples `valid-names/rename`, and `invalid-names/rename-*`.
5. Spec and decision 33 updated for rename.

### Step 3: docs

1. `/review-specs` over the touched specs, then correct them.
2. `CHANGELOG.md`.
3. Move the task folder to `docs/tasks/done/` when finished, and update links.

## Decisions of the owner

1. **Direction of `rename`.** Confirmed: each entry is `NAME_HERE = "NAME_THERE"`. The key is the name written in the
   importing project, and the value is the name that the imported project declares. The task's words "export" and
   "import" are not used, because they read either way. The spec says it with an example, and the comment in the
   example reads `# The name used here = the name in the imported project.`
2. **Full name.** Confirmed: `a.b.c::N` always works for a declared name, hidden or renamed, since it is in effect the
   canonical name. This is rule 6.
3. **Where a clash is reported.** Confirmed: at the later declaration, with the note about hide and rename.
4. **A hidden or renamed name that is not declared.** Confirmed: an error in the project-file. As built in step 1 it
   is found with the other errors of the run, after the files are parsed, so its exit code is 1, not 2. The
   exit codes need a spec of their own, which is not part of this task.
5. **`T?` and `Option`.** Confirmed: nothing is special about `Option`. `T?` is sugar for the bare name `Option`, and
   if that name is hidden or renamed it is unknown, as any other type that is not visible would be. The spec says so.
6. **Granularity.** Confirmed: a name applies to every namespace. There is no finer form.

## Verification

- `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`.
- `bash tests/functest.sh _build/target/debug/constructo`, with the new examples.
- `python3 -I .claude/skills/review-specs/check_specs.py`.
- A run of `constructo check -v --project=...` on the new valid examples, to see the names in the output.
