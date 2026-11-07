# 0030 - Only `project update` fetches, by running `git`, into a cache, 2026-11-04

## Issue

**Fork.** A project from a repository (decision 29) has to be got from somewhere. A repository is somebody else's: it
can hold links to files of the user, a URL that makes git run a program, or hooks. We have to decide when anything is
fetched, how, from where, and how much of a fetched repository is trusted.

## Decision

- Only `constructo project update` goes to the network or writes the cache. `constructo check` never does. It loads each
  project from the commit in the lock, which must be in the cache already. If there is no lock, no entry for the
  project from that repository, or the commit is not in the cache, that is an error at the import with exit code 1 that
  says to run `constructo project update`.
- Fetching runs the `git` program. There is no library for git. A `git` that is not installed is an error with exit
  code 2 that says so. `rev` is resolved by cloning the repository in full and running
  `git rev-parse REV^{commit}`, which also gives the commit of an annotated tag. The files are checked out of the
  commit into a work tree of their own, so that a checkout has no `.git` folder.
- git is run with care:
  - Hooks and submodules are off. Every call passes `-c` settings that make a checkout the same on every machine and run
    no program that a configuration file names (`core.hooksPath=/dev/null`, `core.autocrlf=false`,
    `core.fsmonitor=false` and others). `core.symlinks=true` checks a link out as a link, so that a file system that
    cannot make links fails the checkout, which is an error to see, and does not give a plain file that holds the
    target, which would have a digest that differs from the one made elsewhere.
    The `GIT_*` variables of the environment are removed, because they could redirect git, except `GIT_SSH`,
    `GIT_SSH_COMMAND` and `GIT_ASKPASS`, which say how to reach the repository.
    `GIT_ALLOW_PROTOCOL` is set to `https:ssh:file`, and `GIT_TERMINAL_PROMPT` to `0` so that git never waits for a
    password to be typed.
  - A URL is one of `https://`, `ssh://` or the short form `host:path`, `file://`, or a path. `ext::` and every other
    remote helper, `git://`, `http://` and `ftp://` are refused before git is run, and so is a URL that starts with `-`.
    `--` goes before the URL in every call so that it cannot be taken for an option.
  - A URL with a password in it, or a token as the user name in `https://`, is refused with a message that suggests a
    credential helper or an ssh key. Otherwise the secret would be written in the lock file, in messages and in the
    name of a folder. A user name alone for ssh is fine.
- The cache is `$CONSTRUCTO_CACHE`, or else `constructo` in the cache folder of the user (`$XDG_CACHE_HOME` or
  `~/.cache`, and `%LOCALAPPDATA%` on Windows). A checkout is `git/KEY/COMMIT/`, where KEY is the first 16 hexadecimal
  digits of the SHA-256 of the URL as written. A checkout is made in a folder of its own and renamed into place, and is
  never changed afterwards, so any number of projects share it.
- A fetched project is confined to its clone. The project-file, the folder of the project and every types file must be
  inside the clone once links are followed. A link that leaves it is an error at the import with exit code 2 that says
  where the link leads, and is raised before the project-file is read, so that nothing of the file it leads to can be
  quoted in a message. A `path` import of a fetched project must stay inside the clone by the written path. The built-in
  projects are not subject to this.
- `--dev` (decision 28) is narrowed. It applies to the projects that are folders on disk and are not in a clone, because
  it exists so that editing a local file needs no `update`, and a fetched project cannot be edited. A project from a
  repository, and one that a fetched project imports by `path`, is always loaded from the commit in the lock and always
  verified. So under `--dev` the lock is still read when there is a `repo` import, and a lock that cannot be read is
  exit code 2 then, and not otherwise.

## Consequences

- `check` works offline, and in a pipeline it cannot be made to fetch.
- A first run on a new machine needs `constructo project update` before `check` can pass.
- The whole repository is cloned, which costs time and space for a large one. The alternative of asking for one commit
  does not work for every server or for every kind of `rev`.
- Behaviour depends on the `git` that is installed. The settings above keep it small.
- Windows is handled in the paths of the cache and `file://` URLs but is not tested.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Fetching in `check` when the cache has not got the commit. It is smoother, but gives `check` a network failure mode,
  and breaks the rule that it changes nothing.
- A Rust library for git (`gix` or `git2`). It is self-contained, but a large dependency with its own handling of
  credentials, and the dependencies are to be audited before 1.0.
- Allowing any URL that git accepts. An untrusted project-file could then run a program with `ext::`.
- Following links in a fetched repository, as for a local project. A link out of a clone has a target that is missing or
  is not the author's, and could make the tool read a file of the user's.
