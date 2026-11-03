# Plan: Step 6, remote projects

## Context

Steps 0 to 5 of the projects task are done. Step 6 lets an `[[import]]` take its project from a git repository at a
pinned revision. It comes before Steps 7 and 8, because the specs, the README, the CHANGELOG and the decisions all have
to describe it.

## Decisions settled in the Q&A

1. **Syntax.** `source` is replaced by exactly one of `path`, `repo`, `builtin` on the `[[import]]`. `repo` takes
   optional `rev` and `subdir`. Strict: `rev` or `subdir` without `repo`, two source keys, or none, are errors at the
   import. Existing examples, specs and functests that use `source = ...` are migrated (pre-release, so there is no
   compatibility burden).
2. **Revision.** `rev` is a tag, branch or commit. `constructo project update` resolves it to a full commit hash and
   records it in `constructo.lock`; `check` uses the locked commit only, so a moved tag is caught. No `rev` means the
   repository's default branch, resolved at update time and then pinned.
3. **Fetching.** Only `constructo project update` touches the network or writes the cache. `check` never does. If the
   locked commit is not in the cache, `check` fails with exit 1 and says to run `project update`. `--dev` does not make
   `check` fetch.
4. **Mechanism.** Shell out to `git`, with no new dependency. A clear exit-2 error if `git` is missing. Commands run
   with hooks and submodule recursion off (`core.hooksPath=/dev/null`, no `--recurse-submodules`) and a scrubbed
   `GIT_*` environment. The configuration of the user is not trusted either: every call passes `-c` settings for
   `core.autocrlf=false`, `core.fsmonitor=` and `protocol.file.allow=always`, so a checkout is the same on every
   machine and runs no program that a config file names. A URL that starts with `-` is rejected, and `--` goes before
   the URL in every call, so that a URL cannot be taken as an option.
   A URL with a password in it, as in `https://user:token@host/repo`, is rejected with a message that suggests a
   credential helper or an ssh key. The lock file, verbose output and the cache key would otherwise carry the secret.
   A user name alone is fine.
   `rev` is resolved by a full clone and `git rev-parse <rev>^{commit}`. `ls-remote` cannot resolve a bare or
   abbreviated commit, and an annotated tag has a tag object that is not the commit, so the `^{commit}` is what makes
   the pin a commit in every case.
5. **Transports.** https, ssh and local paths (`file://` or a plain path). `ext::` and other remote helpers are
   rejected before git is run, and `GIT_ALLOW_PROTOCOL` is set to `https:ssh:file`.
6. **Nesting.** A fetched project may import other repos, built-ins, and `path` imports that stay inside the clone.
   The path of the import is joined to the folder of the importing project, `.` and `..` are removed lexically, and
   the result must still start with the clone root, with no links resolved. A `path` that leaves the clone that way is
   an error at the import. Links are dealt with by the check on files in item 9, which uses canonical paths, so an
   imported folder that is a link out of the clone fails there.
7. **Cache.** `$CONSTRUCTO_CACHE`, else the OS user cache directory (`$XDG_CACHE_HOME` or `~/.cache`) plus
   `constructo/git/<sha256 of url, 16 hex>/<commit>/`. A checkout is immutable once made (made in a temporary
   directory, then renamed) and is shared across projects.
8. **Identity.** A remote project's identity is `url@commit` plus `subdir`, replacing the canonical folder used for
   local ones. The same project name from two identities is the existing "project in two places" error.
9. **Subdir.** Optional; the project folder is `<clone>/<subdir>`. The links-out rule in Step 6 of `projects.md` is
   checked against the clone root, so a file may link elsewhere within the clone but not outside it. The project-file
   is held to the same rule.
10. **`--dev`.** It applies to local projects only, because it exists so that editing a local file does not need an
    `update` each time, and a fetched project cannot be edited. A remote project is always loaded from its locked
    commit and always verified, whatever `--dev` says. So under `--dev` the lock is still read for the remote projects:
    a `repo` import that is not in the lock, a missing commit in the cache and a digest that differs are errors as
    without it, and the entries of local projects are ignored. A project with only local imports still needs no lock
    file under `--dev`. In verbose mode a remote project always says `verified`. The lock is read before the projects
    are loaded, since the loader needs the commits, and the digests are compared afterwards as now.


## Changes by area

- `src/project.rs`: `ImportTable` gets `path`, `repo`, `rev`, `subdir` and `builtin` (each `Option<Spanned<String>>`),
  validated to exactly one source. `Import` gains a `Source` enum (`Path`, `Builtin`, `Repo { url, rev, subdir }`).
  `Loader::visit` resolves a `Repo` to its cache checkout using the lock, and `Loader.done`, `names` and `stack` key on
  an `Identity` enum (`Folder(PathBuf)` or `Remote { url, commit, subdir }`). The existing `is_builtin_source` and
  `BUILTIN_SCHEME` are reused for `builtin`.
- New `src/fetch.rs`: a `git()` wrapper, URL validation, `cache_dir()`, `resolve_rev(url, rev) -> commit`,
  `checkout(url, commit) -> PathBuf` (clone into a temporary directory, `git checkout --detach`, rename), the escape
  check, and errors as `ProjectError::Cannot` (exit 2).
- `src/verify.rs`: `LockedProject` gains `repo`, `commit` and `subdir` for remote projects, as keys of the format that
  stays at `version = 1`, since it has not been released. `lock_for`, `compare` and `describe_changes` report a
  moved commit. `update_lock` is the one place that calls `fetch`: it resolves each repo
  import, fetches, then loads and digests as today.
  `read_lock` supplies the pinned commits to the loader used by `check`.
- `src/main.rs`: `--dev` is passed to verification, which skips the local projects only. `project update` needs no
  new flags. Verbose output (Step 5) shows
  `OK: <url>@<short commit> (project com.example.lib, verified)` for a remote project.
- File digests are of the checkout contents, with the existing digest code unchanged.

## Documentation

- Decision 0028 and the `--dev` text in `lock-file-format.md` and `config-file.md` say that `--dev` leaves remote
  projects verified. This is the narrowing that 0028 expected.
- This plan, and the Step 6 section of `projects.md`.
- Decisions 0029 (git shell-out, transports and cache) and 0030 (commit pinning and the source keys).
- `project-file-format.md`, `lock-file-format.md` and `resolving-identifiers.md` for the new keys and identity, as
  Step 7 will check.

## Implementation order

Each is its own commit, with the tests green.

1. Migrate `source` to `path`, `builtin` and `repo` in the parser and in all examples, specs and functests. `repo`
   parses but is rejected as not yet supported.
2. `fetch.rs`: URL validation, cache path and the git wrapper, with unit tests against local bare repos made by `git`
   in a temporary directory.
3. Lock format additions, and `project update` for remote imports.
4. `check` using locked commits and the cache, with the missing-cache error and identity handling.
5. The links-out and `path`-escape checks for fetched projects.
6. Verbose output, the decisions and the spec updates.

## Verification

- Unit tests: URL validation (accepts https, ssh, file and plain paths; rejects `ext::`), cache path stability,
  identity rules, and import-table validation errors with line and column.
- Functests build a local bare repo in `_build/functest-fixtures` with `git init --bare`, a commit and a tag, then
  check that:
  - `update` writes the commit and the digests.
  - `check` passes offline from the cache.
  - A tag moved after `update` is not followed by `check`, but `update` reports the move.
  - A tampered cache file gives a checksum mismatch.
  - A symlink that escapes the clone is refused, and so is a `path` that escapes it.
  - `check` with an empty cache says to run `update`.
  - A missing `git` binary (PATH cleared) gives exit 2.
- `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check` and
  `bash tests/functest.sh _build/target/debug/constructo`.

## Open points

- Settled: the lock `version` stays 1, because the format has not been released.
- The Windows cache directory (`%LOCALAPPDATA%`) and the `file://` path forms are handled in `cache_dir()` but are
  not tested here.
