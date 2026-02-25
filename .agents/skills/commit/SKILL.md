---
name: commit
description: Create a git commit following project conventions. Use when the user asks to commit changes or to rewrite a commit message.
---

Create a git commit for the current staged or unstaged changes.

## Commit message format

- The first line must be an imperative sentence (for example, "Add user authentication", not "Added user authentication" or "Adds user authentication")
- The first line must not end with punctuation
- Do not use conventional commit prefixes like "fix:", "feat:", "chore:", and similar tags
- Additional lines after the first are freeform and can elaborate as needed

## Example

```text
Add endpoint for deleting user accounts

This also updates the admin dashboard to show a confirmation dialog before
deletion, and logs the event for audit purposes.
```
