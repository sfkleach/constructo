# The cache

`constructo` keeps what it downloads in a cache, so that `constructo check` can work without the network and
so that a project from a git repository is fetched once. This document says what is in the cache, how several
instances of `constructo` that run at the same time are kept from harming one another, and how what a run that was
killed leaves behind is cleared away. The projects themselves are described in
[the project-file](project-file-format.md) and the lock in [the lock file](lock-file-format.md).

## Where it is

The cache is a folder. It is the first of these that is given:

1. `--cache=FOLDER` on the command line, before or after the subcommand.
2. `cache` in the `[global]` table of the [configuration file](config-file.md). A relative folder is relative to the
   folder of the configuration file, so the file means the same wherever it is used from.
3. `$CONSTRUCTO_CACHE`, which must be an absolute path. A relative one is an error with exit code 2, since it would
   depend on the folder that the run is in.
4. `constructo` in `$XDG_CACHE_HOME`, or in `$HOME/.cache` if that is not set (on Windows, in `%LOCALAPPDATA%`). A
   relative one of these is ignored, as the XDG specification says.

The first two say in the invocation where the cache is, and are the ones to prefer, in CI especially. The variables are
there so that a cache can be shared without being named each time. A variable that is empty counts as not set, and an
empty folder in the first two is an error. A relative `--cache` is made absolute against the current folder, so that
`--print-config` writes a folder that means the same from anywhere. If no folder can be found, a command that needs the
cache stops with exit code 2 and says how to give one. Nothing about the cache is looked at unless a project from a
repository is reached. Only `constructo project update` writes to the cache. `constructo check` only reads it, and
never changes anything in it.

## What is in it

```text
CACHE/git/KEY/COMMIT/          the files of one commit of one repository
CACHE/git/tmp-PID-TIME-COUNT/  a folder that a run is working in, or one that a killed run left
```

- `KEY` is the first 16 hexadecimal digits of the SHA-256 of the URL of the repository as it is written in the
  `repo` of the import. Two spellings of one repository have two keys, and are two entries.
- `COMMIT` is the full commit, 40 hexadecimal digits (64 for a repository that uses SHA-256).
- The folder of a commit holds the files of the commit and nothing else. It has no `.git` folder and no history.
- Nothing is ever fetched twice into one place. A commit is the same files for ever, so its folder is not replaced
  or updated.

The `COMMIT` folders are the only thing that other runs rely on. A name that is not a well-formed commit is never
turned into a path, so a lock file cannot name a place outside the cache.

## Isolation between instances

Any number of runs may share one cache at once, such as parallel jobs of a build. There is no lock file and no server.
The rules below make the cases that can arise safe without one.

- **A run works in a folder of its own.** `update` makes a folder `git/tmp-PID-TIME-COUNT` with the process, the time
  in nanoseconds and a count in its name, and does everything in it: the clone, the finding of the commit and the
  checkout. If the name is taken already, the run takes another and does not touch the one that is there, since it may
  belong to a run that is still going. In a container that shares a cache, two processes can have one process number,
  which is why the time is in the name too.
- **A checkout appears whole or not at all.** The files are checked out into the folder of the run, and then
  the folder is renamed to `git/KEY/COMMIT`. The scratch folder is in the cache, so it is on the same file system and
  the rename is one step. A reader sees no folder, or the finished one, and never half of one.
- **Two runs of one commit do not clash.** If both finish, one rename succeeds and the other finds the folder there.
  That run takes the folder of the first, which is the same commit and so the same files, and throws its own away.
- **A checkout is not trusted just because it is there.** When `update` finds a folder for the commit it checks out
  the commit afresh as well, and compares the two: every path, the digest of every file and the target of every link.
  A difference is an error with exit code 2 that names the file and the folder, and nothing is written to the lock.
  `update` does not repair the cache, so that a person knows that it was changed. Remove the folder and run `update`
  again. Without this a file that was changed in the cache would be recorded in the lock as a part of the commit.
- **`check` reads the cache and is checked by the lock.** It takes the commit from the lock, finds the folder, and
  compares the digests of the files with the lock as it does for any project. A file that has been changed in the
  cache is a checksum mismatch, and a commit that is not there is an error that says to run `update`.
- **The checkout does not depend on the user.** `git` is run with settings of its own, and the configuration of the user
  is left out of the steps that read and check out the clone. See decision 30. So the configuration of one user cannot
  make a commit come out differently from the same commit in the cache of another.

The cache is trusted as far as the permissions of the folder allow: a program that can write to it can act as the user
anyway. The lock file is what detects a change, and the comparison with a fresh checkout is what stops one from being
recorded.

## Relics of a crash

A run that is killed, or a machine that is switched off, leaves a `tmp-` folder in `git/`. It is no use to anyone, and
it can be large, since it holds a full clone. Nothing else is left, because the folders of the commits are made by one
rename.

- **A run removes its folder when it is done.** It is removed when the run ends, by any route, including an error.
- **A run that is going shows that it is.** While a folder is in use, a thread changes the modified time of the
  folder every hour. So the time of a folder that is in use is never more than about an hour old, however long the
  fetch takes.
- **A fetch sweeps.** At the start, `update` removes the folders in `git/` whose names start with `tmp-` and whose
  modified time is more than 24 hours old, with what is in them. A folder that was changed in the last day may
  belong to a run that is going and is left. Nothing whose name does not start with `tmp-` is removed, and neither is
  a plain file. Failing to remove a folder is not an error, since the sweep is only a tidy-up.
- **Only `update` sweeps.** `check` never changes the cache, so it sweeps nothing. A relic stays until a later
  `update` uses that cache.

## What is not done

- There is no limit on the size of the cache, and no removal of the folders of old commits. A commit that a lock
  names has to stay, and `constructo` cannot tell which locks, in which projects, still name it. To free space, remove
  the folders or the whole cache. The next `update` makes what it needs, and `check` says what is missing.
- There is no lock between runs. A cache on a file system on which a rename of a folder is not one step, such as some
  network file systems, can show a half-made folder to a reader. Use a cache on a local disk, or one cache for each
  job.
- The age of a folder is read from the clock of the machine, so a clock that is set far back keeps a relic for longer.
  A clock that jumps forward by a day while a fetch is going can get its folder swept, which makes that run fail with
  an error that `update` can be run again after. It does not harm a checkout.
- The repository is cloned in full by every `update`, even if the commit is in the cache, since the commit has to be
  looked up and the checkout compared. The cache saves the checkout and the network for `check`, not the clone for
  `update`.

## Related

- [The lock file](lock-file-format.md), for what `update` records and what `check` verifies.
- [The project-file](project-file-format.md), for the `repo`, `rev` and `subdir` of an import.
- [How git is run](git-management.md), for the commands that fill the cache.
- Decisions 29 and 30, in `docs/decisions/`.
