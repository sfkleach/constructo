# Task: Write names through a group in `convert`

## Overview

When `convert` writes Constructo text from a typed notation, it writes a name
bare if the root project has it in scope, and otherwise with the full name of its
project (`docs/specs/convert-command.md`, the bullet "A name"). A project that
imports `com.example.ids` into the group `ids` can write `ids::Uuid`, and
`convert` writes `com.example.ids::Uuid`, which is valid and longer.

The `codegen` task decided that its generated writer writes a name **bare, else
through a group, else in full**
([codegen-subcommand-for-rust.md](../2026-11-17-codegen-subcommand-for-rust/codegen-subcommand-for-rust.md),
"The names in the text"). Its [scenario](../2026-11-17-codegen-subcommand-for-rust/scenario-shapes.md)
found that the two write the same text, byte for byte, for the same value, and
this task keeps it so by making `convert` do the same.

## The rule

- A name in scope is written bare, as now.
- Otherwise, a name that the root project can reach through a group is written
  through it, as `ids::Uuid`. The projects of one group share no names
  (decision 32), so it is never ambiguous.
- A name that an import renames is written with its new name, through the group
  as well as bare. A name that an import hides is not visible through that
  import's group, so the group is not a candidate.
- Where the project is in more than one group, the first import in the
  project-file through which the name is visible is the one used, so the output
  is the same every time.
- Otherwise the name is written in full, as now.

## Where

- `Names::show` in `src/resolve.rs` decides the form. It already handles the
  rename, through `visible_name`, for the bare form.
- The same function shows names in diagnostics (`Names::rewrite`, used by the
  messages). Whether messages are to show names through a group too is to be
  decided: it is consistent, and it changes the expected text of tests that
  quote a full name. A separate function for writing is the alternative.
  *Decided:* messages keep the form they have, bare or full, so that a message
  does not depend on the groups of the project. `Resolution::written` and
  `Resolution::write_name` are for writing, and `show` is for messages.
- *Done* as the first commit of step 3 of
  [codegen](../2026-11-17-codegen-subcommand-for-rust/plan-step3.md), whose
  model uses the same rule. A typed `query` writes its names in the same way.
- The writers that use it: the typed JSON reader (`src/typed_back.rs`) and any
  other path that writes Constructo text with names resolved for a project.
  Search for the callers of `show` to find them all.

## Tests

- A project that imports another into a group: `convert --from=json-typed`
  writes `group::Name`, and `check` accepts it in the project.
- A rename through a group, a hidden name, and a project in two groups.
- The existing tests that expect a full name where a group would do, which change.
- The spec, `docs/specs/convert-command.md`, says the new rule.
