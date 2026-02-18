---
name: update-readme
description: Update README.md to reflect changes since it was last updated
---

Update the README.md to reflect any user-facing changes since it was last
modified.

Steps:

1. Find the last commit that touched README.md:
   `git log --oneline --follow -1 README.md`
2. List commits since that commit:
   `git log --oneline <last-readme-commit>..HEAD`
3. For any commits that look user-facing (new commands, flags, config options,
   behavioral changes), read the relevant source to understand the change.
4. Read the current README.md.
5. Edit README.md to incorporate the changes, matching the existing style and
   structure. Only add or modify sections relevant to the changes — do not
   rewrite unrelated sections.
6. If there are no user-facing changes, say so and do nothing.
