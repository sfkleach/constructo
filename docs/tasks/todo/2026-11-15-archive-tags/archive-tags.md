# Task: Mark merged branches with archive tags

## Overview

When a PR merges, its branch is deleted on GitHub, and the local branch is kept. A local branch without its remote is
how a merged branch is recognised, and reading the dead branches is a quick way back into a project after time away,
which may be years.

That marker is local. Work moves between four machines (two Linux mini-PCs, a Linux laptop and a macOS laptop), so each
clone has only the dead branches that were made on it, and a fresh clone has none. Git's own branch descriptions
(`git branch --edit-description`) have the same flaw: they live in `.git/config` and are never pushed.

This task replaces the marker with something that every clone has: an annotated tag on the tip of each merged branch,
pushed to GitHub, whose message says what the branch was for.

## The idea

- When a PR merges, tag the head of its branch as `archive/<branch>`, with an annotated tag. The tag records who made
  it and when, and its message describes the branch: the PR title and number, the merge date, a summary, and the task
  folder under `docs/tasks/done/` where there is one.
- Push the tag. Every clone gets it with `git fetch --tags`, every client lists it, and it lasts as long as the
  repository does.
- A `just archive-branch NAME` recipe does it in one step from any machine. It uses `gh` to find the merged PR whose
  head was the branch, so it does not need the local branch: GitHub keeps the head of every PR as
  `refs/pull/<number>/head` after the branch is deleted.
- Backfill tags for the branches that have merged already. At the time of writing, these are PRs #1 to #24, from
  `check-datatypes` to `jumpstart_rust`; #6, #9 and #10 do not appear among the merged PRs.

## Questions to settle

- **The name.** `archive/<branch>` reads well, but a branch can be reused (`projects`, `projects2`, `projects3` suggest
  this is avoided already) and a branch name may contain a `/` (`add/consts`). Should the PR number be in the name, as
  `archive/23-query_subcommand`, so that names never collide and sort by age?
- **What the message holds.** The PR title and number, the merge date and method (rebase, squash or merge commit), the
  task folder, and a summary. Where does the summary come from: the first paragraph of the PR body, the task's
  overview, or written by hand at the time? The PR body is often long, and a tag message is best when it is short.
- **Which commit is tagged.** The PR head (the branch as it was reviewed) or the commits as they landed on `main`?
  After a rebase merge they differ in hash. Tagging the PR head keeps the branch as it was; the message can name the
  commit on `main` as well.
- **Signing.** Releases use signed tags (`git tag -s` in `draft-release`). Should archive tags be signed too? Signing
  needs the key on whichever machine runs the recipe.
- **The existing tags.** `old_main`, `tmp-step7`, `tmp_latest`, `tmp_origin_step_5`, `tmp_origin_step_7` and
  `was_mixed_files` are already tags. Should they be renamed into a namespace, given archive messages, or left alone?
- **Branches without a PR.** Some dead branches, such as `fuzz-spike`, never had a PR. Should the recipe accept a
  branch and a message for those, tagging the remote branch or the local one?
- **Whether to keep the local branches.** With archive tags, the local dead branches are no longer needed as a marker.
  Keeping them is harmless, and it stays the user's choice. The recipe must not delete any branch, local or remote.
- **How the tags are read.** `git tag -l 'archive/*' -n99` lists them with their messages. Is a `just archived`
  recipe, or a git alias, worth adding to make that the habit?

## Done when

- `just archive-branch` tags and pushes the archive tag for a merged PR's branch, refusing a branch whose PR has not
  merged, and an archive tag that exists already.
- The merged PRs to date have archive tags on GitHub.
- The convention is written down where it will be found: `CLAUDE.md`, and `docs/practice/` next to the definition of
  done, which gains a step to run the recipe after a merge.
