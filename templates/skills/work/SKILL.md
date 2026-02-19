---
name: work
description: List and summarize currently running work sessions. Answer questions about what the user has discussed recently. What tasks are running? What agents are running? What is my current work? Work snapshots. What needs my attention?
---

# work

`work` is a CLI tool that delegates coding tasks to AI agents, each in its own
isolated git worktree. It manages git worktrees and AI agent sessions so you can
describe what you want and let agents solve it in parallel.

You are likely running inside a `work`-managed worktree right now.

## Key concepts

- **Project** -- A registered git repository. All worktrees and sessions for
  that project are tracked together.
- **Task** -- A plain git worktree for manual development, no agent attached.
- **Session** -- An AI agent attempt at solving an issue. Each session gets its
  own worktree and branch, runs an agent, and produces a report.
- **Issue** -- Freeform text describing what needs to be done.

## Session lifecycle

Sessions progress through these statuses:

| Status | Meaning |
|--------|---------|
| `planned` | Created, waiting to start |
| `running` | Agent is actively working |
| `reported` | Agent finished with a report |
| `stopped` | User stopped the agent |
| `failed` | The agent encountered an error |

## Common commands

```bash
# Start a new session
work new "fix the bug"
work new --agents 3 "refactor the query builder"

# List sessions
work list
work list --json

# Show session details
work show <ID>

# Monitor session output
work logs <ID>
work logs <ID> --follow

# Stop or delete a session
work stop <ID>
work delete <ID>

# Open a session's PR
work pr <ID>

# cd into a session's worktree
work open <ID>
```

### Tasks (manual worktrees)

```bash
work task new                     # Create a task (auto-named)
work task new my-feature          # Create a named task
work task list                    # List tasks
work cd my-task                   # cd to a task's worktree
work cd                           # cd to the project root
work task delete my-task          # Delete a task
```

### Projects

```bash
work projects create              # Register current directory as a project
work projects list                # List all projects
work projects delete <name>       # Delete a project
```

### Diagnostics

```bash
work doctor                       # Check system health
work tree                         # Show tree of projects, tasks, and sessions
work tui                          # Launch interactive dashboard
```

## Tips for AI agents

- If you are running inside a `work` session, your worktree is isolated. You
  can make changes freely without affecting the main branch.
- The session report (written to `.work/session-report.md`) is read by the user
  to understand what you did. Write clear, concise reports.
- Use `work list --json` or `work show <ID>` for structured information about
  sessions.
- The daemon must be running for most commands. If commands fail, suggest
  `work daemon start --detach` or `work doctor` to diagnose issues.
