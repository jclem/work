# work

A CLI for managing work. It runs a local daemon that manages projects,
environments, and tasks, with a terminal UI for monitoring.

## Install

### Homebrew

```bash
brew install jclem/tap/work
```

### From source

```bash
cargo install --git https://github.com/jclem/work
```

## Usage

Start the daemon, then interact with it through the CLI or TUI.

```bash
work daemon start
```

### Projects

Register a project from the current directory:

```bash
work project new
```

Or specify a name and path:

```bash
work project new my-project --path /path/to/project
```

```bash
work project ls
work project rm my-project
```

### Environments

Environments are isolated workspaces for tasks. The built-in environment
provider is `git-worktree`, which creates git worktrees.

```bash
work env create                    # create and claim an environment
work env prepare                   # create a pooled environment
work env claim <id>                # claim a pooled environment
work env update <id>               # update a pooled environment
work env ls
work env rm <id>
work env provider ls               # list available providers
```

### Tasks

Tasks run commands in environments. Task providers are configured in the config
file.

```bash
work task new "fix the login bug"  # create and start a task
work task new "add tests" --attach # create and follow logs
work task ls
work task rm <id>
```

### Logs

```bash
work logs <task-id>
work logs <task-id> -f             # follow in realtime
```

### TUI

```bash
work tui
```

## Configuration

Edit the config file:

```bash
work config edit
```

The config lives at `$XDG_CONFIG_HOME/work/config.toml` (typically
`~/.config/work/config.toml`).

```toml
default-environment-provider = "git-worktree"
default-task-provider = "my-provider"

[tasks.providers.my-provider]
type = "command"
command = "claude"
args = ["-p", "$DESCRIPTION", "--workdir", "$WORKTREE_PATH"]
```

## Shell completions

```bash
work completions <bash|zsh|fish|elvish|powershell>
```

## License

MIT
