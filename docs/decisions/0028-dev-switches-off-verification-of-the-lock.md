# 0028 - `--dev` switches off verification of the lock, 2026-11-01

## Issue

**Fork.** With decision 27 a change to an imported project is an error until `project update` is run. Someone who is
developing a library and the project that uses it changes both all the time, and does not want to update the lock after
each edit. We have to decide how they say so.

## Decision

- `-d, --dev` switches off all verification against the lock, for every project. A missing lock file is then not an
  error either, and a lock that does not parse is not read.
- It is a setting like the other booleans (decision 21): `dev` in the `[check]` table of a settings file, a negative
  form `--no-dev`, the last of the two on the command line winning, and it is shown by `--print-config`. It is off by
  default.
- The setting is not in the project-file, and the settings file is read only when it is named. So a project cannot
  switch off the checking of what it imports, and whoever runs the check on a project chooses.
- Verification happens only when a project is named, after the projects are loaded and before any file is read, so a
  mismatch stops the run before values are checked.

## Consequences

- Development on a local dependency needs no lock file and no updates.
- One flag covers everything, so a run with `--dev` verifies nothing, including imports that are not being developed.
  Decision 30 has narrowed this for remote projects: a project from a repository is always verified.
- Verbose mode will say that projects are `unverified`, so that a run with `--dev` is not mistaken for a verified one.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- A `dev` key in the project-file. The project would then decide for everyone that uses it, and a dependency could
  turn off the checking of itself.
- Making a missing lock a warning. A warning is ignored in a pipeline, and the lock would protect nobody.
- Updating the lock automatically when it differs. See decision 27: it would hide the changes that it exists to expose.
