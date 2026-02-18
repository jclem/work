---
name: commit
description: Create git commits following this project's commit message conventions. Use when asked to commit changes.
user_invocable: true
---

# Commit

Create git commits for staged or unstaged changes in the repository.

## Commit message format

- First line: one imperative sentence.
- Capitalize first word.
- No trailing punctuation.
- Avoid prefixes like `feat:`, `fix:`, `chore:`.
- Optional body: include concise detail about what changed and why.
- End each commit message with a co-author trailer:
  `Co-Authored-By: Claude <{{email}}>`

## Workflow

1. Inspect current state with `git status` and `git diff` (staged and unstaged).
2. Check recent history with `git log --oneline -5` for local style consistency.
3. Split unrelated changes into separate focused commits.
4. For each commit:
- Stage only relevant files by name.
- Commit with a well-formed message (heredoc is preferred for multi-line messages).
- Verify final state with `git status`.
