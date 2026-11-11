# llamatop — shared rules for agents

This file is read by OpenCode (AGENTS.md) and by Claude Code (CLAUDE.md, a
symlink to this file). Single source of truth: only this file is edited.

## Roles

- **OpenCode**, on the aibox's local model: writes the code. In a job, it
  works alone in a worktree, on a `gen/<id>-*` branch.
- **Claude Code**: reviews, fixes, decides. Nothing reaches `main` without
  review: automatic merge when the checks and Claude Code's review pass,
  otherwise by Florian (`dev merge`).

## Language

- Everything written for an agent (this file, task sheets, briefs) is in
  English. Code, comments and commit messages too.
- Your final report is in **French**: Florian reads it.

## Rules

- Never `git push`, never rewrite history, never commit on `main`.
- Small, atomic commits, messages in English: `type: description`
  (feat, fix, refactor, test, docs, chore).
- Before finishing: if `.devbox/check` exists, run `bash .devbox/check`
  and fix until it passes.
- No dependency added without a justification in the commit message.
- Never build artifacts, binaries or large files in git.
- Ambiguity: pick the simplest, reversible option and explain it in the
  commit.

## Project

<!-- To fill in: stack, layout, commands, project-specific conventions. -->

- Stack:
- Run in dev:
- Tests:
- Conventions:
