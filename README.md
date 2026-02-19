# work

Delegate coding tasks to AI agents, each in its own isolated git worktree. No
stashing, no branch juggling, no waiting.

`work` manages git worktrees and AI agent sessions so you can describe what you
want and let agents solve it in parallel.

```
$ work new "fix the off-by-one error in the pagination logic"
✓ Started session 42 (attempt 1) on branch 2026-02-19-bold-lark
```

## Highlights

- **Session-based workflow** -- describe an issue and spawn parallel agent sessions
- **Isolated worktrees** -- every session and task gets its own directory and branch
- **Parallel agents** -- start multiple attempts on the same issue with `--agents N`
- **Interactive TUI** -- monitor all sessions, view logs, and manage projects from a dashboard
- **Pre-warmed pool** -- large repos stay fast because worktrees are created in the background
- **Post-creation hooks** -- run setup scripts (`npm install`, etc.) automatically after creating a worktree
- **Resource-aware** -- pool pre-warming backs off when CPU or memory is under pressure

## Install

**With [mise](https://mise.jdx.dev):**

```bash
mise use -g github:jclem/work
```

**From source:**

```bash
cargo install --path .
```

## Quick start

```bash
# Initialize your shell (add to your shell config)
eval "$(work init zsh)"   # or bash, fish

# Start the daemon
work daemon start --detach

# On macOS, install as a Launch Agent for auto-start:
work daemon install

# Register a project
cd ~/src/my-project
work projects create

# Start your first session
work new "add input validation to the signup form"
```

## Recommended workflow

The fastest way to use `work` day-to-day:

### 1. Keep the TUI open

Bind `work tui` to a hotkey in your terminal or window manager so you always
have a dashboard one keypress away. The TUI auto-refreshes and shows every
session across all projects.

```bash
work tui          # launch the TUI dashboard
work ui           # alias
```

### 2. Start sessions with `work new`

Describe what you want done. The agent handles branching, coding, and opening a
draft PR.

```bash
work new "fix the login redirect after password reset"
```

For hard problems, run multiple agents in parallel and let them race:

```bash
work new "refactor the query builder to support joins" --agents 3
```

If you omit the issue text, `work new` opens your `$EDITOR` so you can write a
longer description. You can also pipe from stdin:

```bash
gh issue view 42 --json body -q .body | work new
```

### 3. Monitor progress in the TUI

Switch to the TUI to watch sessions as they work. Key actions:

| Key | Action |
|-----|--------|
| `Enter` | View session details and report |
| `Ctrl+L` | View live session logs |
| `Ctrl+P` | Open the session's PR in a browser |
| `x` | Stop a running session |
| `s` | Start a new session |

### 4. Clean up

```bash
work delete 43     # remove a session and its worktree
```

## Concepts

### Projects

A **project** is a registered git repository. Register one with
`work projects create` from inside the repo (or pass a path). All worktrees and
sessions for that project are tracked together.

### Tasks

A **task** is a plain git worktree for manual development -- no agent attached.
Create one with `work task new` when you want to work on something yourself.
Each task gets an auto-generated name like `2026-02-19-bold-lark` (or you can
choose your own) and a dedicated branch.

### Sessions

A **session** is an AI agent attempt at solving an issue. Sessions are the core
unit of `work`. Each session:

- Gets its own worktree and branch
- Runs an agent that writes code and opens a draft PR
- Produces a report summarizing what was done
- Has a status: `planned` → `running` → `reported`

Multiple sessions can target the same issue (parallel attempts). Review them
in the TUI.

### Issues

An **issue** is freeform text describing what you want done. It can be a
sentence, a paragraph, or a GitHub issue body piped from `gh`. Issues are
stored with their sessions, not as standalone entities.

## Shell setup

`work` needs a shell wrapper to `cd` into new worktrees. Add one of these to
your shell config:

**fish** (in `~/.config/fish/config.fish`):
```fish
work init fish | source
```

**zsh** (in `~/.zshrc`):
```zsh
eval "$(work init zsh)"
```

**bash** (in `~/.bashrc`):
```bash
eval "$(work init bash)"
```

For completions:

```bash
eval "$(work completions zsh)"    # or bash, fish
```

## Usage

### Sessions

```bash
# Start sessions
work new "fix the bug"            # Start a session (aliases: start, create)
work new                          # Opens $EDITOR for the issue description
work new --agents 3               # Start 3 parallel sessions
work new --project my-project     # Target a specific project

# List and inspect
work list                         # List sessions in the current project
work list --all                   # List sessions across all projects
work list --json                  # JSON output
work ls                           # Alias for list
work show 42                      # Show session details and report
work tree                         # Show a tree of all projects, tasks, and sessions

# Monitor
work logs 42                      # View session output
work logs 42 --follow             # Tail output in real time (like tail -f)

# Act on results
work stop 42                      # Stop a running session
work pr 42                        # Open the session's PR in a browser
work open 42                      # cd into the session's worktree

# Clean up
work delete 42                    # Delete session and its worktree
```

### Tasks

Tasks are worktrees for manual work, not agent sessions.

```bash
work task new                     # Create a task (auto-named)
work task new fix-login           # Create a named task
work task new -b existing-branch  # Use an existing branch
work task new --no-cd             # Create without cd-ing into it

work task list                    # List tasks in the current project
work task list --all              # List tasks across all projects

work task delete fix-login        # Delete a task
work task rm fix-login --force    # Force delete with uncommitted changes

work cd my-task                   # cd to a task's worktree
work cd                           # cd to the project root
```

### Projects

```bash
work projects create              # Register current directory
work projects create /path/to/repo --name my-project

work projects list                # List all registered projects
work projects ls --json           # JSON output

work projects delete my-project   # Delete project and its worktrees
```

### TUI dashboard

```bash
work tui                          # Launch the TUI (alias: work ui)
work tui --interval 10            # Custom refresh interval (seconds)
```

The TUI has three tabs:

**Sessions** -- View and manage all agent sessions.

| Key | Action |
|-----|--------|
| `s` | Start a new session |
| `Enter` | View session details |
| `Ctrl+L` | View session logs |
| `Ctrl+P` | Open PR in browser |
| `x` | Stop session |
| `d` | Delete session |
| `` ` `` | Toggle tree/flat view |
| `e` | Toggle empty projects |

**Tasks** -- View and manage worktrees.

| Key | Action |
|-----|--------|
| `n` / `c` | Create new task |
| `d` / `Delete` | Delete task |
| `N` | Nuke all tasks |
| `P` | Clear pool worktrees |

**Daemon** -- View daemon status and control it.

| Key | Action |
|-----|--------|
| `s` | Start daemon |
| `x` | Stop daemon |
| `R` | Restart daemon |

**Global keys:**

| Key | Action |
|-----|--------|
| `?` | Toggle help overlay |
| `Tab` / `]` / `[` | Switch tabs |
| `1` / `2` / `3` | Jump to tab |
| `↑` `↓` / `k` `j` | Navigate |
| `F5` | Refresh |
| `q` / `Esc` | Quit |

### Daemon

```bash
work daemon start                 # Start (foreground by default)
work daemon start --detach        # Run in the background
work daemon start --force         # Replace an already-running daemon
work daemon stop                  # Stop
work daemon restart               # Restart
work daemon pid                   # Print PID
work daemon socket-path           # Print socket path
work daemon install               # Install as a Launch Agent (macOS)
work daemon uninstall             # Uninstall the Launch Agent
```

### Diagnostics

```bash
work doctor                       # Check system health (database, daemon, projects, sessions)
work tree                         # Show a tree of all projects, tasks, and sessions
```

## Configuration

Global config lives at `~/.config/work/config.toml`. Per-project config lives
at `.work/config.toml` in the project root (takes priority).

```toml
# ~/.config/work/config.toml

# Default branch for new worktrees (default: "main")
default-branch = "main"

# Per-project settings
[projects.my-large-repo]
default-branch = "develop"
pool-size = 2                     # Pre-warm 2 worktrees (default: 0, disabled)

[projects.my-large-repo.hooks]
new-after = """
#!/bin/bash
npm install
"""

# Daemon resource thresholds
[daemon]
pool-max-load = 0.7              # Back off when load > 70% of CPU count
pool-min-memory-pct = 10         # Back off when available memory < 10%
pool-poll-interval = 300         # Seconds between pool checks (default: 5 min)
```

Per-project `.work/config.toml` uses the same keys without the
`[projects.<name>]` wrapper:

```toml
# my-project/.work/config.toml
default-branch = "develop"
pool-size = 3

[hooks]
new-after = """
#!/bin/bash
npm install
"""
```

Open the config in your editor:

```bash
work config edit
```

See [docs/CONFIGURATION.md](docs/CONFIGURATION.md) for the full configuration
reference, including all daemon, orchestrator, TUI, and environment variable
settings.

## How it works

### Session lifecycle

When you run `work new "fix the bug"`:

1. The issue text is captured (from the argument, `$EDITOR`, or stdin)
2. A session record is created in the database with status `planned`
3. The daemon picks up the session, creates a worktree and branch, and spawns an agent
4. The agent works in the worktree: reads code, makes changes, opens a draft PR
5. When the agent finishes, the session moves to `reported` with a summary
6. You review the result and manage the session as needed

Session statuses:

| Status | Meaning |
|--------|---------|
| `planned` | Created, waiting to start |
| `running` | Agent is actively working |
| `reported` | Agent finished with a report |
| `stopped` | You stopped the agent |
| `failed` | The agent encountered an error |

### Worktrees

Worktrees live at `$XDG_DATA_HOME/work/<project>/worktrees/<task>/`
(default: `~/.local/share/work/...`). Each session and task gets a worktree
with its own branch, completely isolated from your main checkout.

### Pre-warm pool

In large repos, `git worktree add` is slow. The pool system pre-creates
worktrees in the background so they're ready when you need them. Pre-warming is
opt-in: set `pool-size` in your config to enable it.

The daemon fills the pool:
- On startup
- After a session or task claims a pool entry
- Every `pool-poll-interval` seconds (default: 5 min)

Pool worktrees use temporary `__pool-*` branch names. When claimed, the branch
is renamed and the directory is moved -- no extra `git worktree add` needed.

See [docs/POOL.md](docs/POOL.md) for full details.

### Daemon

The daemon is an HTTP server over a Unix domain socket. It runs background
workers for:

- **Session management** -- spawning and monitoring agent processes
- **Deletion** -- processing `work delete` requests asynchronously
- **Pool maintenance** -- keeping pre-warmed worktrees filled, respecting CPU and memory thresholds

Socket location:
- `$XDG_RUNTIME_DIR/work/workd.sock` (when `XDG_RUNTIME_DIR` is set)
- `/tmp/work/workd.sock` (fallback)

Override with `work daemon start --socket /path/to/workd.sock` or the
`WORKD_SOCKET_PATH` environment variable.

## File locations

| File | Default path |
|------|-------------|
| Database | `~/.local/share/work/database.sqlite` |
| Global config | `~/.config/work/config.toml` |
| Daemon socket | `$XDG_RUNTIME_DIR/work/workd.sock` |
| Daemon PID | `$XDG_RUNTIME_DIR/work/workd.pid` |
| Daemon log | `$XDG_RUNTIME_DIR/work/workd.log` |
| Worktrees | `~/.local/share/work/<project>/worktrees/<task>/` |

All paths respect `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, and `XDG_RUNTIME_DIR`.

## Output conventions

- Machine-readable values (e.g. `daemon socket-path`) print to **stdout**
- Status messages, warnings, and errors print to **stderr**
- Use `--json` for structured output, `--plain` for tab-separated values

## Development

```bash
cargo build                       # Build
cargo test                        # Run tests
cargo fmt --check                 # Check formatting
cargo clippy --all-targets --all-features -- -D warnings  # Lint
```

Or with [mise](https://mise.jdx.dev):

```bash
mise run check                    # Run all checks (fmt + clippy)
mise run test                     # Run tests
mise run dev                      # Start the daemon in dev mode
mise run fix                      # Auto-fix formatting and lint issues
mise run release:local            # Build release binary and install to /usr/local/bin
```
