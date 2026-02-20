# Configuration

`work` uses TOML configuration files with XDG-compliant paths. All
configuration is optional -- the CLI works out of the box with sensible
defaults.

## Table of contents

- [Config files](#config-files)
  - [Global config](#global-config)
  - [Per-project config](#per-project-config)
  - [Resolution order](#resolution-order)
  - [Editing the config](#editing-the-config)
- [General settings](#general-settings)
  - [default-branch](#default-branch)
  - [task-name-command](#task-name-command)
- [Per-project settings](#per-project-settings)
  - [pool-size](#pool-size)
  - [default-branch (per-project)](#default-branch-per-project)
  - [task-name-command (per-project)](#task-name-command-per-project)
  - [hooks](#hooks)
- [Daemon settings](#daemon-settings)
  - [pool-max-load](#pool-max-load)
  - [pool-min-memory-pct](#pool-min-memory-pct)
  - [pool-poll-interval](#pool-poll-interval)
  - [pool-pull-enabled](#pool-pull-enabled)
  - [pool-pull-interval](#pool-pull-interval)
  - [pr-cleanup-enabled](#pr-cleanup-enabled)
  - [pr-cleanup-interval](#pr-cleanup-interval)
- [Named orchestrators](#named-orchestrators)
- [Orchestrator settings](#orchestrator-settings)
  - [default](#default)
  - [agent-command](#agent-command)
  - [system-prompt](#system-prompt)
  - [max-agents-in-flight](#max-agents-in-flight)
  - [max-sessions-per-issue](#max-sessions-per-issue)
- [TUI settings](#tui-settings)
  - [refresh-interval](#refresh-interval)
- [Environment variables](#environment-variables)
- [File locations](#file-locations)
- [UI state](#ui-state)
- [Full example](#full-example)

---

## Config files

### Global config

The global configuration file lives at:

```
$XDG_CONFIG_HOME/work/config.toml
```

which defaults to `~/.config/work/config.toml`.

This file holds top-level defaults, per-project overrides, daemon tuning, and
orchestrator settings.

### Per-project config

Each project can have its own config at:

```
<project-root>/.work/config.toml
```

This file uses the same keys as the `[projects.<name>]` section of the global
config, but without the `[projects.<name>]` wrapper. It is checked into version
control alongside the project.

### Resolution order

Settings are resolved from most specific to least specific:

1. **Per-project config** (`.work/config.toml` in the project root) -- highest priority
2. **Global per-project config** (`[projects.<name>]` in the global config)
3. **Global defaults** (top-level keys in the global config)
4. **Built-in defaults** -- lowest priority

For example, the effective `default-branch` for a project named `my-project` is
determined by checking, in order:

```toml
# 1. my-project/.work/config.toml
default-branch = "staging"            # checked first

# 2. ~/.config/work/config.toml
[projects.my-project]
default-branch = "develop"            # checked second

# 3. ~/.config/work/config.toml (top-level)
default-branch = "trunk"              # checked third

# 4. built-in default: "main"         # checked last
```

The first value found wins.

### Editing the config

Open the global config in your `$EDITOR`:

```bash
work config edit
```

This creates the file if it does not exist.

---

## General settings

Top-level keys in the global config.

### `default-branch`

The branch that new worktrees (and pool worktrees) are created from.

| | |
|---|---|
| **Key** | `default-branch` |
| **Type** | string |
| **Default** | `"main"` |
| **Scope** | global, per-project |

```toml
# ~/.config/work/config.toml
default-branch = "main"
```

### `task-name-command`

A script body used to generate task and worktree names. The value is written to
a temporary file and executed directly, so you can use any interpreter via a
shebang line (e.g. `#!/usr/bin/env fish`). Scripts without a shebang are
executed by the OS default (`/bin/sh` on Unix).

The script's trimmed stdout is used as the task/branch name. If the script
fails or produces empty output, the built-in `YYYY-MM-DD-adjective-noun`
generator is used as a fallback.

Two environment variables are set before the script runs:

| Variable | Value |
|---|---|
| `WORK_PROJECT` | The project name |
| `WORK_ISSUE` | The issue description (only set for sessions started with `work new`) |

| | |
|---|---|
| **Key** | `task-name-command` |
| **Type** | string (script body) |
| **Default** | none (built-in adjective-noun generator) |
| **Scope** | global, per-project |

```toml
# ~/.config/work/config.toml
task-name-command = """
#!/bin/sh
date +%Y-%m-%d-$(openssl rand -hex 4)
"""
```

Example using an LLM to generate names from the issue text:

```toml
task-name-command = """
#!/bin/sh
echo "$WORK_ISSUE" | llm -s 'Output a short kebab-case branch name. No explanation.'
"""
```

Example using fish:

```toml
task-name-command = """
#!/usr/bin/env fish
set date (date +%Y-%m-%d)
echo "$date-"(random choice cool neat wild)"-"(random choice cat fox owl)
"""
```

---

## Per-project settings

Per-project settings can appear in two places: under `[projects.<name>]` in the
global config, or as top-level keys in the project's `.work/config.toml`.

### `pool-size`

Number of pre-warmed worktrees to maintain for a project. Set to `0` or omit
to disable pre-warming entirely. See [docs/POOL.md](POOL.md) for details on
how the pool works.

| | |
|---|---|
| **Key** | `pool-size` |
| **Type** | integer |
| **Default** | `0` (disabled) |
| **Scope** | per-project |

Global config:

```toml
[projects.my-large-repo]
pool-size = 2
```

Project-level `.work/config.toml`:

```toml
pool-size = 3
```

### `default-branch` (per-project)

Override the default branch for a specific project. Takes precedence over the
global `default-branch`.

```toml
[projects.my-project]
default-branch = "develop"
```

Or in `.work/config.toml`:

```toml
default-branch = "develop"
```

### `task-name-command` (per-project)

Override the task name generation script for a specific project. Takes
precedence over the global `task-name-command`. See
[`task-name-command`](#task-name-command) for full details.

```toml
[projects.my-project]
task-name-command = """
#!/bin/sh
echo "my-project-$(date +%Y%m%d)-$(openssl rand -hex 3)"
"""
```

Or in `.work/config.toml`:

```toml
task-name-command = """
#!/bin/sh
echo "$(date +%Y-%m-%d)-custom"
"""
```

### Hooks

Hooks are shell scripts that run at specific lifecycle points. Currently, one
hook is supported:

#### `new-after`

Runs after a task worktree is created (whether from the pool or freshly via
`git worktree add`). Use it to bootstrap the working environment for the
project -- install dependencies, build artifacts, set up tooling, etc.

| | |
|---|---|
| **Key** | `hooks.new-after` |
| **Type** | string (shell script) |
| **Default** | none |
| **Scope** | per-project |

The script runs with its working directory set to the new worktree. It is
executed as a shell script (via the system shell), so you can use any shell
syntax.

Global config:

```toml
[projects.my-project.hooks]
new-after = """
#!/bin/bash
npm install
"""
```

Project-level `.work/config.toml`:

```toml
[hooks]
new-after = """
#!/bin/bash
npm install
cp .env.example .env
"""
```

**Note:** Hooks never run during pool pre-warming. They only run interactively
during `work new` / `work task new`, so the user always sees hook output.

---

## Daemon settings

All daemon settings live under the `[daemon]` section of the global config.
These control the background daemon's behavior for pool management, branch
updating, and PR cleanup.

The daemon re-reads the config file on each worker cycle, so changes take
effect without restarting.

### `pool-max-load`

Maximum CPU load average (as a fraction of CPU count) before the daemon stops
creating pool worktrees. For example, on a machine with 8 CPUs, the default of
`0.7` means pool creation pauses when the 1-minute load average exceeds 5.6.

| | |
|---|---|
| **Key** | `daemon.pool-max-load` |
| **Type** | float |
| **Default** | `0.7` |

```toml
[daemon]
pool-max-load = 0.5   # more conservative: back off above 50% of CPU count
```

### `pool-min-memory-pct`

Minimum percentage of available memory before the daemon stops creating pool
worktrees. When available memory drops below this threshold, pool creation
pauses until memory is freed.

| | |
|---|---|
| **Key** | `daemon.pool-min-memory-pct` |
| **Type** | float |
| **Default** | `10.0` |

```toml
[daemon]
pool-min-memory-pct = 15   # more conservative: need at least 15% free
```

### `pool-poll-interval`

Seconds between periodic pool maintenance checks. On each tick, the daemon
checks all projects for pool deficits and creates worktrees as needed (subject
to resource thresholds).

| | |
|---|---|
| **Key** | `daemon.pool-poll-interval` |
| **Type** | integer (seconds) |
| **Default** | `300` (5 minutes) |

```toml
[daemon]
pool-poll-interval = 600   # check every 10 minutes instead
```

### `pool-pull-enabled`

Whether to periodically pull (update) the default branch in pool worktrees so
they stay current. When enabled, pool worktrees are kept up to date with
remote, so `work new` starts from a recent commit.

| | |
|---|---|
| **Key** | `daemon.pool-pull-enabled` |
| **Type** | boolean |
| **Default** | `true` |

```toml
[daemon]
pool-pull-enabled = false   # disable pool pulls entirely
```

### `pool-pull-interval`

Seconds between pool pull cycles. Only applies when `pool-pull-enabled` is
`true`.

| | |
|---|---|
| **Key** | `daemon.pool-pull-interval` |
| **Type** | integer (seconds) |
| **Default** | `3600` (1 hour) |

```toml
[daemon]
pool-pull-interval = 1800   # pull every 30 minutes
```

### `pr-cleanup-enabled`

Whether to automatically delete sessions whose pull requests have been merged
or closed. When enabled, the daemon periodically checks PR state via the GitHub
CLI (`gh`) and cleans up completed sessions.

| | |
|---|---|
| **Key** | `daemon.pr-cleanup-enabled` |
| **Type** | boolean |
| **Default** | `true` |

```toml
[daemon]
pr-cleanup-enabled = false   # keep sessions around after PR merge/close
```

### `pr-cleanup-interval`

Seconds between PR cleanup sweeps. Only applies when `pr-cleanup-enabled` is
`true`.

| | |
|---|---|
| **Key** | `daemon.pr-cleanup-interval` |
| **Type** | integer (seconds) |
| **Default** | `300` (5 minutes) |

```toml
[daemon]
pr-cleanup-interval = 600   # sweep every 10 minutes
```

---

## Named orchestrators

Named orchestrators let you define reusable orchestrator configurations under
`[orchestrators.<name>]` in the global config. Each definition can include an
`agent-command` and `system-prompt`. You can then reference them by name
instead of repeating the full command everywhere.

```toml
# Define named orchestrators
[orchestrators.claude]
agent-command = [
  "claude", "-p",
  "--dangerously-skip-permissions",
  "--system-prompt", "{system_prompt}",
  "{issue}",
]

[orchestrators.codex]
agent-command = [
  "codex", "exec",
  "--dangerously-bypass-approvals-and-sandbox",
  "{system_prompt}\n\n---\n\n{issue}",
]
```

Once defined, reference them by name:

- **As the global default:** set `default = "claude"` under `[orchestrator]`
- **Per-project:** set `orchestrator = "codex"` under `[projects.<name>]`

```toml
[orchestrator]
default = "claude"       # all projects use "claude" unless overridden

[projects.my-frontend]
orchestrator = "codex"   # this project uses "codex" instead
```

### Resolution for `agent-command` and `system-prompt`

When determining the effective `agent-command` or `system-prompt`, the
following resolution order applies (first value found wins):

1. **Per-project config** (`.work/config.toml` `[orchestrator]` section, or
   `orchestrator = "<name>"` referencing a named orchestrator)
2. **Global per-project config** (`[projects.<name>.orchestrator]` inline table,
   or `[projects.<name>] orchestrator = "<name>"`)
3. **Global orchestrator config** (`[orchestrator]` inline `agent-command` /
   `system-prompt`, or the named orchestrator referenced by `default`)
4. **Built-in defaults**

Inline values always take precedence over named references at the same level.

---

## Orchestrator settings

The orchestrator controls how agent sessions (started with `work start`) are
executed. Settings can be defined globally under `[orchestrator]`, per-project
under `[projects.<name>.orchestrator]` in the global config, or under
`[orchestrator]` in a project's `.work/config.toml`.

Resolution order follows the same pattern as other settings: project-level
`.work/config.toml` > global per-project > global orchestrator > built-in
defaults.

### `default`

Name of a named orchestrator (from `[orchestrators]`) to use as the global
default. When set, the named orchestrator's `agent-command` and
`system-prompt` are used unless overridden by inline values.

| | |
|---|---|
| **Key** | `orchestrator.default` |
| **Type** | string (orchestrator name) |
| **Default** | none |
| **Scope** | global |

```toml
[orchestrator]
default = "claude"
```

### `agent-command`

The command template used to launch an agent session. This can be either:

- An argv array (first element is the binary, remaining elements are args)
- A script body string that is written to a temporary executable file and run
  directly (use a shebang such as `#!/usr/bin/env fish`)

Four placeholders are available and will be replaced at runtime:

| Placeholder | Replaced with |
|---|---|
| `{issue}` | The issue description text |
| `{issue_id}` | Extracted issue identifier (for example `ABC-123`) when present in `{issue}`; empty otherwise |
| `{system_prompt}` | The effective system prompt |
| `{report_path}` | Path where the agent should write its report |

Placeholders can be embedded inside larger strings (for example
`"foo_{issue_id}"`).

When using script form, these environment variables are also set for the
script process:

| Variable | Value |
|---|---|
| `WORK_SESSION_ID` | Numeric session ID |
| `WORK_SESSION_ISSUE` | Session issue text |
| `WORK_SESSION_ISSUE_ID` | Extracted issue identifier (or empty) |
| `WORK_SESSION_SYSTEM_PROMPT` | Effective system prompt |
| `WORK_SESSION_WORKTREE` | Session worktree path |
| `WORK_SESSION_PROJECT` | Project name |
| `WORK_SESSION_BASE_SHA` | Base commit SHA |
| `WORK_SESSION_REPORT_PATH` | Report output path |

| | |
|---|---|
| **Key** | `orchestrator.agent-command` |
| **Type** | array of strings OR script string |
| **Default** | `["claude", "-p", "--dangerously-skip-permissions", "--disallowedTools", "EnterPlanMode", "--system-prompt", "{system_prompt}", "{issue}"]` |
| **Scope** | global, per-project |

Can be set inline or inherited from a named orchestrator via `default`:

```toml
# Inline argv form (takes precedence):
[orchestrator]
agent-command = ["claude", "-p", "--system-prompt", "{system_prompt}", "{issue}"]

# Inline script form:
[orchestrator]
agent-command = """
#!/usr/bin/env fish
codex exec --json --dangerously-bypass-approvals-and-sandbox \
  "{system_prompt}\n\n---\n\n{issue}"
"""

# Or via named orchestrator:
[orchestrator]
default = "claude"   # uses [orchestrators.claude].agent-command
```

Per-project override (inline or by name):

```toml
# By name:
[projects.my-project]
orchestrator = "codex"

# Or inline:
[projects.my-project.orchestrator]
agent-command = ["custom-agent", "--prompt", "{issue}"]

# Or inline script:
# [projects.my-project.orchestrator]
# agent-command = """
# #!/usr/bin/env fish
# custom-agent --prompt "{issue}" --report "{report_path}"
# """
```

### `system-prompt`

A custom system prompt injected into the agent command via the
`{system_prompt}` placeholder. Use this to give agents project-specific
instructions, coding standards, or context.

| | |
|---|---|
| **Key** | `orchestrator.system-prompt` |
| **Type** | string |
| **Default** | none (a built-in prompt is used) |
| **Scope** | global, per-project |

```toml
[orchestrator]
system-prompt = "You are working on a Rust CLI project. Follow the project's coding conventions."
```

### `max-agents-in-flight`

Maximum number of agent sessions that can run concurrently across all projects.
The daemon uses a semaphore to enforce this limit -- additional sessions queue
until a slot opens.

| | |
|---|---|
| **Key** | `orchestrator.max-agents-in-flight` |
| **Type** | integer |
| **Default** | `4` |
| **Scope** | global |

```toml
[orchestrator]
max-agents-in-flight = 8   # allow more concurrent agents
```

### `max-sessions-per-issue`

Maximum number of sessions that can be created for a single issue. Prevents
runaway session creation.

| | |
|---|---|
| **Key** | `orchestrator.max-sessions-per-issue` |
| **Type** | integer |
| **Default** | `5` |
| **Scope** | global |

```toml
[orchestrator]
max-sessions-per-issue = 10
```

---

## TUI settings

Settings for the interactive terminal UI (`work tui` / `work ui`).

### `refresh-interval`

Auto-refresh interval in seconds for the TUI dashboard. Can also be overridden
at runtime with the `--interval` flag.

| | |
|---|---|
| **Key** | `tui.refresh-interval` |
| **Type** | integer (seconds) |
| **Default** | `5` |

```toml
[tui]
refresh-interval = 10   # refresh every 10 seconds
```

CLI override:

```bash
work tui --interval 2
```

---

## Environment variables

| Variable | Purpose | Default |
|---|---|---|
| `XDG_CONFIG_HOME` | Base directory for the global config file | `~/.config` |
| `XDG_DATA_HOME` | Base directory for the database and worktrees | `~/.local/share` |
| `XDG_STATE_HOME` | Base directory for the UI state file | `~/.local/state` |
| `XDG_RUNTIME_DIR` | Base directory for the daemon socket, PID, and log | System temp dir |
| `WORKD_SOCKET_PATH` | Override the daemon socket path entirely | (derived from `XDG_RUNTIME_DIR`) |
| `NO_COLOR` | Disable colored output when set (any value) | unset |
| `EDITOR` | Editor opened by `work config edit` | -- |
| `HOME` | Home directory fallback when XDG vars are not set | -- |
| `WORK_PROJECT` | Set by `task-name-command` scripts: the project name | -- |
| `WORK_ISSUE` | Set by `task-name-command` scripts: the issue description (sessions only) | -- |
| `WORK_SESSION_ID` | Set for agent-command execution: numeric session ID | -- |
| `WORK_SESSION_ISSUE` | Set for agent-command execution: issue text | -- |
| `WORK_SESSION_ISSUE_ID` | Set for agent-command execution: extracted issue identifier | -- |
| `WORK_SESSION_SYSTEM_PROMPT` | Set for agent-command execution: effective prompt | -- |
| `WORK_SESSION_WORKTREE` | Set for agent-command execution: worktree path | -- |
| `WORK_SESSION_PROJECT` | Set for agent-command execution: project name | -- |
| `WORK_SESSION_BASE_SHA` | Set for agent-command execution: base commit SHA | -- |
| `WORK_SESSION_REPORT_PATH` | Set for agent-command execution: report path | -- |

---

## File locations

All paths follow the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/).

| File | Default path |
|---|---|
| Global config | `~/.config/work/config.toml` |
| Database | `~/.local/share/work/database.sqlite` |
| UI state | `~/.local/state/work/state.toml` |
| Daemon socket | `$XDG_RUNTIME_DIR/work/workd.sock` |
| Daemon PID file | `$XDG_RUNTIME_DIR/work/workd.pid` |
| Daemon log | `$XDG_RUNTIME_DIR/work/workd.log` |
| Project worktrees | `~/.local/share/work/projects/<project>/worktrees/<task>/` |
| Pool worktrees | `~/.local/share/work/projects/<project>/worktrees/__pool-<id>/` |
| Per-project config | `<project-root>/.work/config.toml` |

When `XDG_RUNTIME_DIR` is not set, the daemon falls back to the system
temporary directory (e.g. `/tmp/work/`).

---

## UI state

A small state file at `~/.local/state/work/state.toml` (or
`$XDG_STATE_HOME/work/state.toml`) tracks UI preferences that persist across
TUI sessions. This is managed automatically by the TUI and is not typically
edited by hand.

| Key | Type | Default | Purpose |
|---|---|---|---|
| `show-empty-projects` | boolean | `false` | Whether the TUI shows projects with no tasks |

---

## Full example

A complete global config file showing all available settings:

```toml
# ~/.config/work/config.toml

# Global default branch for new worktrees
default-branch = "main"

# Custom task/branch name generator (optional)
# task-name-command = """
# #!/bin/sh
# echo "$WORK_ISSUE" | llm -s 'Output a short kebab-case branch name. No explanation.'
# """

# ─── Named orchestrator definitions ─────────────────────────────────

[orchestrators.claude]
agent-command = [
  "claude", "-p",
  "--dangerously-skip-permissions",
  "--system-prompt", "{system_prompt}",
  "{issue}",
]

[orchestrators.codex]
agent-command = [
  "codex", "exec",
  "--dangerously-bypass-approvals-and-sandbox",
  "{system_prompt}\n\n---\n\n{issue}",
]

# ─── Per-project settings ───────────────────────────────────────────

[projects.frontend]
default-branch = "develop"
pool-size = 3
orchestrator = "codex"                    # use codex by name

[projects.frontend.hooks]
new-after = """
#!/bin/bash
npm install
cp .env.example .env
"""

[projects.backend]
default-branch = "main"
pool-size = 2

[projects.backend.hooks]
new-after = """
#!/bin/bash
cargo build
"""

# ─── Daemon tuning ──────────────────────────────────────────────────

[daemon]
pool-max-load = 0.7
pool-min-memory-pct = 10
pool-poll-interval = 300
pool-pull-enabled = true
pool-pull-interval = 3600
pr-cleanup-enabled = true
pr-cleanup-interval = 300

# ─── Orchestrator defaults ──────────────────────────────────────────

[orchestrator]
default = "claude"                        # global default orchestrator
max-agents-in-flight = 4
max-sessions-per-issue = 5
# system-prompt = "..."                   # uncomment to set a global system prompt

# ─── TUI ────────────────────────────────────────────────────────────

[tui]
refresh-interval = 5
```

And a matching per-project config:

```toml
# my-project/.work/config.toml

default-branch = "develop"
pool-size = 2

# Custom branch naming for this project (optional)
# task-name-command = """
# #!/bin/sh
# date +%Y-%m-%d-$(openssl rand -hex 4)
# """

[hooks]
new-after = """
#!/bin/bash
mise install
cargo build
"""

# Reference a named orchestrator from the global config:
orchestrator = "codex"

# Or override inline:
# [orchestrator]
# system-prompt = "Follow the project's CLAUDE.md for coding conventions."
```
