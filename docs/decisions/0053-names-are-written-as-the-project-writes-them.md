# 0053 - Names are written as the project writes them, 2026-11-25

## Issue

A constructor can be written bare, through a group, or with the full name of its project, which is valid in any
project ([decision 42](0042-a-value-can-name-any-project-by-its-full-name.md)). The full name is long: the prelude's
`Some(1)` is `constructo.prelude::Some(1)`, and the prefix is on every constructor. The codec of
[decision 51](0051-a-codec-for-each-project-holding-its-names-consts-and-settings.md) writes the names of a value, and
reads the names of a text. The question was which form it writes, and which it reads.

This is a **Leap**, from the draft of the codegen task, written after the code. The task
`convert-writes-names-through-a-group`, done first, made `convert` write names in the same way.

## Factors

- What the codec writes should be accepted by `check` in the project that the code was generated for, which is the
  oracle of its tests.
- Text that goes to a party whose imports are not known can only rely on full names.
- A project can put another into several groups, and an import can rename or hide a name.
- The same project should always write the same text for the same value.

## Decision

**A codec writes each name as the root project writes it,** by default: bare if the name resolves to that constructor
in the root's scope, else through the first group in the project-file through which it is visible, else in full. A
name that an import renames is written with its new name, and a group that hides it is not used. The form is worked
out by the resolver when the code is generated, and is the one that `convert` writes.

**A codec whose `names` setting is `Full` writes every name in full,** which any project reads. Without a project the
two are the same, since a name has nothing to be relative to.

**A codec reads every form that a values file of the root project reads, and no other:** the full name of a project of
the run, the name through each group whose scope has it, `local` among them, and the bare name if `local` has it, each
the new name where an import renames it. The model of the declarations holds the forms of each constructor and const,
and they are worked out by trying each candidate with the resolver that `check` uses, and keeping those that resolve to
it. Each form is turned into the full name, which is the key of the codec's tables.

**The writer never writes a reference to a const.** It writes the value, and every argument of a tuple, by name if the
parameter has one, including those equal to their defaults, so that the text does not depend on the defaults.

## Consequences

- What a codec writes is what `convert --from=json-typed` writes for the same value in the same project, and `check`
  accepts it there. The harness holds the two together.
- Text that is written with the project's names may not be read by another project with other imports. A program that
  sends text elsewhere sets the codec's names to `Full`.
- A change to the imports of the root project can change the text that the codec writes, but not what it means.
