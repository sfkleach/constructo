# 0057 - The command is named codegen, 2026-11-26

## Issue

The command that writes the types and the codec of a project in another language needed a name. It sits beside
`check`, `convert`, `query` and `project`, each of which is a word that says what it does.

This is a **Fork**, between `codegen` and `generate`, recorded after the command was built. The draft of the codegen
task chose the name.

## Factors

- `constructo` also converts and queries data, so a name that could mean making data would mislead.
- The other commands are short, and are typed often.
- What the command writes is code, in a language that `--lang` names.

## Decision

**The command is `constructo codegen`.** It says that what is written is code, which `generate` does not. In a tool
that also converts data, `generate` could be read as making sample values, or values from the types.

## Consequences

- If the command ever writes something that is not code, such as a schema or documentation, the name is to be looked
  at again.
- `codegen` is a word of programmers' jargon, and not of English. Those who use the command are programmers.

## Options and Outcome

- **`codegen`:** chosen.
- **`generate`:** an ordinary word, which does not say what is generated.
