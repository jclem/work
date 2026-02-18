---
name: github-actions
description: Guidelines for managing GitHub Actions workflow dependencies with pinned SHAs. Use when adding, updating, or reviewing action references in workflow files.
---

# GitHub Actions

## Why pin action references to commit SHAs

GitHub Actions `uses:` references like `actions/checkout@v4` point to mutable tags. A compromised or force-pushed tag could silently change CI behavior. Pinning to a commit SHA makes references immutable.

```yaml
# Mutable tag (unsafe):
- uses: actions/checkout@v4

# Pinned SHA with version comment (safe):
- uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v4.x.y
```

## gh-actions-versions

Use the `gh-actions-versions` GitHub CLI extension to manage pinned SHAs.

### Installation

```bash
gh extension install jclem/gh-actions-versions
```

### Commands

```bash
gh actions-versions fix
gh actions-versions verify
gh actions-versions update --all
gh actions-versions update actions/checkout
gh actions-versions upgrade actions/checkout --version v7
gh actions-versions upgrade --all
```

## Workflow for adding or changing an action

1. Add or update the `uses:` line with the desired version tag.
2. Run `gh actions-versions fix` to pin to exact SHAs.
3. Run `gh actions-versions verify` to ensure references are valid.