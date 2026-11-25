# 0050 - A target writes one file or a folder, 2026-11-25

## Issue

`constructo codegen` writes code for a target language. Rust's modules can all be in one file, but a module of Python
is a file of its own, so a run with imported projects writes several files, which have to be a package. The question
was how the command is told where to write, so that a path is never taken for the other kind of output, and how what
an earlier run wrote is replaced.

This is a **Leap**, from the draft of the codegen task, written when the first target, Rust, was built. Rust writes
only a file; the rules for a folder are written down now so that the next target does not decide them again.

## Factors

- A path such as `out` could be meant as a file or as a folder, and a command that guessed would sometimes write a file
  where a folder was meant, or the other way round.
- A package that a run replaces can hold a module that a later run no longer writes, such as that of a project that is
  not imported any more, so it has to be replaced as a whole and not file by file.
- A folder that a user points at by mistake may hold their own files, which must never be removed.
- A run that fails part of the way through, or is stopped, must not leave a file or a package that is part old and part
  new, which a build would take for code.

## Decision

- **A target writes one file or a folder, according to its language,** and the option says which is meant:
  `--out-file=FILE_PATH` or `--out-dir=FOLDER`. A target that writes one file writes to the standard output when
  neither is given.
- **The wrong option is a usage error,** exit code 2, which names the right one: `Rust writes one file: use
  --out-file=FILE_PATH, or neither for the standard output`. Both options at once are one too.
- **A file is replaced at once:** it is written to a new file beside it, which then takes its place by a rename, so that
  the file is always the old one or the new one.
- **A folder belongs to the generator.** A run writes only to a folder that does not exist, is empty, or has the marker
  file that `codegen` writes in every folder that it makes, and refuses any other with exit code 2, changing nothing in
  it. The package is written to a new folder beside it, which then takes its place.
- **Nothing is written if there is an error,** in the inputs or in the code, since every file is made in memory first.

## Consequences

- A user who gives the wrong kind of path is told which to give, and nothing is written where it was not meant.
- A folder of a package can be regenerated without the user removing it first, and a file of the user's is never
  removed.
- The marker file is the one thing in a package that is not code. Its name and content are decided by the first target
  that writes a folder.
- `--out-dir` exists for Rust only to be refused, so that the message is the same for every target.
