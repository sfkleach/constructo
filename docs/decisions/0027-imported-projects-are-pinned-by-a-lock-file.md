# 0027 - Imported projects are pinned by a lock file, 2026-11-01

## Issue

**Fork.** A project imports other projects (decision 24), and whatever is in the imported folder is what gets used. If
a dependency changes, the importing project can change meaning, or be tampered with, without anyone noticing. We have
to decide what is recorded to prevent that, where, how it is checked and how it is brought up to date.

## Decision

- A project that imports other projects has a lock file, `constructo.lock`, in its root folder, next to
  `constructo.toml`. It is TOML, with `version = 1` and one `[[project]]` for each project that is loaded, directly or
  not, other than the root project and the built-in ones. Each has its `name`, a `digest` and a table of the digest of
  each of its files. The format is in `docs/specs/lock-file-format.md`.
- The files that are covered are the types files that match the project's patterns, and its `constructo.toml`. Values
  files and other files are not covered.
- A digest is `sha256:` and 64 lowercase hexadecimal digits. A file's digest is that of its bytes after removing one
  leading byte order mark and replacing each CRLF with a LF in one pass, so that it does not depend on the platform or
  the editor. The digest of a project is that of the lines `PATH DIGEST` of its files, in order of the bytes of the
  paths, each ended by a LF.
- A file is identified by its path from the project folder, written with `/`, and its digest is of the content it has.
  A symbolic link inside or out of a local project is fine, since the content is what is pinned. Refusing links out of a
  fetched project is in decision 30.
- The lock must describe exactly what is loaded. `check` reports, all together and with exit code 1: a digest that
  differs, a loaded project or file that is not in the lock, and a project or file in the lock that is not loaded. A
  problem is reported at the place that explains it: a missing project at the `[[import]]` that first loaded it, and
  the rest at the entry in the lock. Each message says to run `constructo project update` where that is the cure.
- A lock file that cannot be read or does not parse is exit 2, as a bad project-file is. The reader is strict: unknown
  or missing keys, a wrong type, another version, a bad name or path and a digest that does not match its files are all
  errors.
- A project with nothing to record needs no lock file, and a run with no `--project` has none. Any other project with
  no lock is an error at its first import.
- `constructo check` never writes a file. `constructo project update [FOLDER]` does: it loads the project as `check`
  does, writes the lock, and prints one line for each project or file that was added, changed or removed, with the old
  and new digest. It type-checks nothing and does not need the old lock to be valid, since its purpose is to accept
  what is on disk. If there is nothing to record it deletes any old lock instead of leaving it stale.
- The source of a project is not recorded. The same project may be written with different paths by different importers.

## Consequences

- A changed dependency is an error until the change is accepted by running `project update`, and the diff of
  `constructo.lock` is what a reviewer reads.
- A project's digests are the same wherever it is and on every platform, so a lock made on one machine is valid on
  another.
- Every project that imports another needs a lock file, and has to run `project update` after changing a dependency.
  `--dev` (decision 28) is the way to work without that.
- Only what is covered is pinned. A change to a values file is not noticed.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- A digest in each `[[import]]` of the project-file. It would put generated content in a file that people write, and
  would pin only the direct imports, not the ones they bring in.
- Pinning only the digest of a project and not of its files. A mismatch could then say only that something changed. The
  digests of the files cost little and let the message name the file.
- Digests of the raw bytes. A checkout that converts line endings, or an editor that adds a byte order mark, would then
  break the lock for no real change.
- Refusing a path that, once links are resolved, leaves the project folder. It was tried and dropped: the lock digests
  the content anyway, and a link out of a local project is legitimate, for example to cross-reference projects.
- A command that updates the lock as a part of `check`. A check that changes files would hide exactly the changes that
  the lock exists to expose.
