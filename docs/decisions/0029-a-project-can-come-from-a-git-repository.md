# 0029 - A project can come from a git repository, 2026-11-04

## Issue

**Fork.** Until now an import names a folder on disk. To share a library between projects without copying it, an import
has to be able to name a version of a project in a git repository. We have to decide how the import says so, which
revision it means, and what is recorded to make the same revision come back each time.

## Decision

- An `[[import]]` says where the project is with exactly one of `path`, `builtin` or `repo`. This replaces the single
  key `source`. A `repo` may have a `rev` and a `subdir`. Two of the three, none, or a `rev` or `subdir` without a
  `repo`, is an error at the import. The project-file is still strict.
- `rev` is anything that git resolves: a tag, a branch or a commit. Without one it is the head of the default branch.
- What is pinned is the commit. `constructo project update` resolves the `rev` to the full commit hash and writes the
  repository, the commit and the `subdir` in the entry for the project in `constructo.lock` (decision 30 says how the
  lock is used). The `rev` is not written there, and `update` looks it up again each time, which is how a branch is
  brought forward. `check` uses the commit and ignores the `rev`, so a tag that is moved is not followed.
- The identity of a project is still the canonical path of its folder. For a project from a repository that is its
  checkout in the cache, whose path is made of the repository, the commit and the `subdir`. Two checkouts that give one
  project name are the existing error that a project is in two places.
- A project from a repository may import other repositories, built-in projects, and projects by `path`, if the path
  stays inside the clone. A project that is reached by such a path stays inside the clone too.

## Consequences

- A fetched project can be reproduced exactly, and a repository that is changed, retagged or rewritten later cannot
  change what a locked project means.
- `source` no longer exists. This is before the first release, and every example, spec and test was changed.
- The lock records a repository URL, so the URL is part of what is committed. The same project fetched from a mirror is
  a different entry, which is the safe reading.
- A `rev` that is a branch needs `project update` to be run to see new commits. That is the intention.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Keeping `source` and adding `repo` beside it. Then "exactly one of `source` or `repo`" is a weaker shape, and the
  meaning of `source` is split between a folder and a URI of a built-in project.
- A `[import.source]` sub-table. It is the most extensible and the most verbose.
- Pinning a commit hash only in the project-file. It is immutable but nobody writes a hash by hand, and a tag cannot
  then be used at all.
- Recording the `rev` in the lock. It adds nothing that `check` could use, and would suggest that the lock decides what
  the `rev` means.
