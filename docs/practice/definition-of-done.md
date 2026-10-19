# Definition of Done

## Task (feature) criteria, maintained at end of task

### Documentation consistency

- All documents are reviewed for consistency and inconsistencies should be corrected. Documents are normally in `docs/` and task-documents are in `docs/tasks/`. Task documents that are superseded by later changes do not need correction; it is sufficient to mark them as superseded.
- The `CHANGELOG.md` must reflect the new feature.
- The grammars in `syntax/`, the examples in `syntax/examples/` and the
  documents in `docs/` agree with each other and with the implementation.
- Design decisions made during the task are recorded in `docs/decisions/`.
- The task document is moved to `docs/tasks/done/`.

### Automated testing

- The complete set of tests must pass. These can be automatically run using `just test`, which is responsible for running unit tests, functional tests, formatting checks, checking all executables build, and running security checks.
- If the crate gains feature flags, the `test` recipe in `Justfile` must try building with all combinations of them.

## Step (story) criteria, constantly maintained

The comment, programming-style and developer-documentation guidelines in
[CLAUDE.md](../../CLAUDE.md) apply to every step. They are maintained there, and
not repeated here, so that there is a single source.
