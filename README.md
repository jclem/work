# work

Isolated git worktrees for every task. No more stashing, no more branch juggling.

`work` manages git worktrees so each task gets its own directory and branch. A
background daemon pre-warms worktrees in large repos so `work new` is instant.

```
$ work new
Created task 2026-02-17-bold-fox
~/worktrees/my-project/2026-02-17-bold-fox $
```

## Highlights

- **Instant task creation** -- pre-warmed worktree pool means no waiting for `git worktree add` in large repos
- **Auto-generated names** -- tasks get memorable `YYYY-MM-DD-adjective-noun` names (or bring your own)
- **Background cleanup** -- the daemon deletes worktrees and branches asynchronously
- **Post-creation hooks** -- run setup scripts (`npm install`, etc.) automatically after creating a task
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

# Start the daemon (or use `work daemon install` on macOS for auto-start)
work daemon start

# Register a project
cd ~/src/my-project
work projects create

# Create your first task
work new
```

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

### Tasks

```bash
work new                          # Create a task (auto-named)
work new fix-login                # Create a named task
work new --project my-project     # Create in a specific project
work new --no-cd                  # Create without cd-ing into it

work list                         # List tasks in the current project
work list --all                   # List tasks across all projects
work list --json                  # JSON output
work ls                           # Alias for list

work delete fix-login             # Delete a task (async, via daemon)
work rm fix-login --force         # Force delete with uncommitted changes

work nuke                         # Remove ALL tasks, projects, and pool entries
```

### Projects

```bash
work projects create              # Register current directory
work projects create /path/to/repo --name my-project

work projects list                # List all registered projects
work projects ls --json           # JSON output

work projects delete my-project   # Delete project and its worktrees
```

### Daemon

```bash
work daemon start                 # Start (daemonizes by default)
work daemon start --attach        # Run in the foreground
work daemon start --force         # Replace an already-running daemon
work daemon stop                  # Stop
work daemon restart               # Restart
work daemon pid                   # Print PID
work daemon socket-path           # Print socket path (for scripting)
work daemon install               # Install as a Launch Agent (macOS)
work daemon uninstall             # Uninstall the Launch Agent (macOS)
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

## How it works

### Worktrees

When you run `work new`, the CLI:

1. Detects the project from your current directory (or `--project` flag)
2. Generates a task name like `2026-02-17-bold-fox`
3. Claims a pre-warmed worktree from the pool (if available), or falls back to `git worktree add`
4. Creates a branch named after the task
5. Runs post-creation hooks
6. Changes your shell directory to the new worktree

Worktrees live at `$XDG_DATA_HOME/work/<project>/worktrees/<task>/`
(default: `~/.local/share/work/...`).

### Pre-warm pool

In large repos, `git worktree add` is slow. The pool system pre-creates
worktrees in the background so they're ready when you need them. Pre-warming is
opt-in: set `pool-size` in your config to enable it.

The daemon fills the pool:
- On startup
- After `work new` claims a pool entry
- Every `pool-poll-interval` seconds (default: 5 min)

Pool worktrees use temporary `__pool-*` branch names. When claimed, the branch
is renamed and the directory is moved -- no extra `git worktree add` needed.

See [docs/POOL.md](docs/POOL.md) for full details.

### Daemon

The daemon is an HTTP server over a Unix domain socket. It runs two background
workers:

- **Deletion worker** -- processes `work delete` requests asynchronously (up to 4 in parallel)
- **Pool worker** -- maintains pre-warmed worktrees, respecting CPU and memory thresholds

Socket location:
- `$XDG_RUNTIME_DIR/workd/workd.sock` (when `XDG_RUNTIME_DIR` is set)
- `/tmp/workd/workd.sock` (fallback)

Override with `work daemon start --socket /path/to/workd.sock` or the
`WORKD_SOCKET_PATH` environment variable.

## File locations

| File | Default path |
|------|-------------|
| Database | `~/.local/share/workd/database.sqlite` |
| Global config | `~/.config/work/config.toml` |
| Daemon socket | `$XDG_RUNTIME_DIR/workd/workd.sock` |
| Daemon PID | `$XDG_RUNTIME_DIR/workd/workd.pid` |
| Daemon log | `$XDG_RUNTIME_DIR/workd/workd.log` |
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
