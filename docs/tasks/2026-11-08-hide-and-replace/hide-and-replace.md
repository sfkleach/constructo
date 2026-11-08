# Task: Implement hidden and renamed variables in imports.

## Overview

An `import` section can provide a list of variables to hide and also provide
a list of renamings. For example, a project file might look like this.

```toml
[project]
name = "com.example.shapes"
include = ["**/*.types"]
exclude = ["old/**"]

[[import]]
project-name = "com.example.base"
group = "base"
path = "../base"
hide = ["Debug", "LegacyPoint"]

[import.rename]
# Format is EXPORT_NAME = IMPORT_NAME, so the EXPORT_NAME is the one that is 
# accessible. And it must be unambiguous - the usual rule.
Point = "BasePoint"
Colour = "BaseColour"

[[import]]
project-name = "constructo.prelude"
group = "local"
builtin = "constructo:/prelude"
```

## Step 1: Hide

Hidden variables are excluded from the import. This is useful when you want to
import into (say) `local` but there are one or two accidental clashes that are
part of the spec you are not using.

- [x] Implement the `hide` field as part of an import.
- [x] Update the project file specification (docs/specs/project-file-format.md).

## Step 2: Rename

Renamed variables are a way of avoiding a name clash in a group by renaming the
clashing variables of an import. The rename must not generate a conflict with 
any other variable in the group.

- [x] Implement the `rename` field as part of an import.
- [x] Update the project file specification (docs/specs/project-file-format.md).

## Step 3: Update docs

- [ ] /review-specs
- [ ] Update the CHANGELOG.md
