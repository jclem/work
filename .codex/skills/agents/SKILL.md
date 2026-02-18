---
name: agents
description: How the .agents/ directory works. Use when adding, editing, or syncing agent configuration, skills, or instructions.
---

# Agent Configuration

All agent instructions and skills are managed centrally in `.agents/` and synced
to tool-specific locations (`.claude/skills/`, `.codex/skills/`, `AGENTS.md`,
`CLAUDE.md`). Never edit the synced files directly — they are overwritten by the
sync script.

## Directory structure

```
.agents/
  config.toml           # template variables
  INSTRUCTIONS.md       # source for AGENTS.md and CLAUDE.md
  skills/
    <skill-name>/SKILL.md
  sync.sh               # render + sync script
```

## Template variables

`.agents/config.toml` defines variables under `[vars]` that can be referenced in
any source file as `{{var_name}}`. The sync script replaces these placeholders
with their values when rendering.

## Adding or editing a skill

1. Create or edit `.agents/skills/<name>/SKILL.md`.
2. Use `{{var}}` syntax for any values that should come from `config.toml`.
3. Run `mise run agents:sync` to render and distribute the skill.

## Adding or editing instructions

1. Edit `.agents/INSTRUCTIONS.md`.
2. Run `mise run agents:sync` to update `AGENTS.md` and `CLAUDE.md`.

## Syncing

- `mise run agents:sync` — render templates and write to all destinations.
- `mise run agents:check` — verify synced files match rendered output (used in pre-commit).

The check runs automatically as part of `mise run pre-commit`.