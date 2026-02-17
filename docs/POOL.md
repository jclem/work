# Pre-warm Worktree Pool

In large repos, `git worktree add` is slow, making `work new` feel sluggish.
The pool system pre-creates worktrees in the background so they're ready when
you need them.

## How it works

The daemon maintains a pool of pre-created git worktrees for each project that
opts in. These worktrees use temporary branch names with a `__pool-` prefix
(e.g. `__pool-a1b2c3d4`) and live in the same `worktrees/` directory as regular
task worktrees.

### When are pool worktrees created?

The daemon's pool worker creates worktrees in three situations:

1. **On daemon startup** -- it immediately checks all projects and fills any
   deficit.
2. **After `work new` claims a pool entry** -- the CLI notifies the daemon to
   replenish.
3. **On a periodic poll** -- the worker wakes every `pool-poll-interval` seconds
   (default 300 / 5 minutes) to check for deficits and pick up config changes.

Worktrees are created one at a time, not in parallel, to be gentle on the
system.

### What happens when the user runs `work new`?

1. The CLI checks the `pool` table for an available pre-warmed worktree for the
   project.
2. **If a pool entry exists**, it claims it by:
   - Atomically deleting the oldest pool row from the database.
   - Renaming the git branch: `git branch -m __pool-a1b2c3d4 2026-02-17-bold-fox`.
   - Moving the worktree to its final path: `git worktree move <old> <new>`.
   - Notifying the daemon to replenish the pool.
3. **If no pool entry exists**, it falls back to normal `git worktree add`,
   exactly as if pooling were not configured. There is no error or warning -- it
   just takes the usual amount of time.

After claiming or creating, the task is inserted into the `tasks` table and
hooks run normally (see below).

### When do hooks run?

Hooks (like `new-after`) **never** run during pool pre-warming. Pre-warming only
runs `git worktree add` with a temporary branch name.

Hooks run during `work new`, after the worktree is claimed from the pool (or
created fresh). This means the user always sees hook output interactively,
regardless of whether the worktree came from the pool.

### How do git branches work?

During pre-warming, the daemon creates a branch with a temporary name like
`__pool-a1b2c3d4` that branches off the configured default branch (see
[Default branch](#default-branch) below).

When `work new` claims a pool entry, it renames the branch to the real task name
(e.g. `2026-02-17-bold-fox`) using `git branch -m`. The worktree directory is
also moved to the final path. From git's perspective, this is the same as if the
worktree had been created fresh -- the branch just has a different name and the
working directory is in a different location.

The `__pool-` prefix makes collisions with real task names (which use
`YYYY-MM-DD-adjective-noun` format) impossible.

## Resource thresholds

Before creating each pool worktree, the daemon checks system resources:

- **Load average**: If the 1-minute load average exceeds `pool-max-load` times
  the number of CPUs, creation is skipped.
- **Available memory**: If available memory drops below `pool-min-memory-pct`
  percent of total memory, creation is skipped.

When either threshold is exceeded, the worker stops creating worktrees for that
cycle and returns. The next poll or notify will try again.

## Configuration

Pre-warming is strictly opt-in. If `pool-size` is omitted or set to `0`, no
pre-warming occurs for that project.

### Per-project pool size

Set in the project's `.work/config.toml`:

```toml
pool-size = 2
```

Or in the global config (`~/.config/work/config.toml`):

```toml
[projects.my-large-repo]
pool-size = 2
```

The project-level file takes priority over the global config.

### Default branch

Pool worktrees (and `work new` worktrees) branch off a configurable default
branch. Set `default-branch` in the project's `.work/config.toml`:

```toml
default-branch = "develop"
```

Or in the global config:

```toml
[projects.my-large-repo]
default-branch = "develop"
```

If not specified, the default is `"main"`.

### Daemon settings

Set in the global config:

```toml
[daemon]
pool-max-load = 0.7       # back off when 1-min load avg > 70% of CPU count
pool-min-memory-pct = 10  # back off when available memory < 10% of total
pool-poll-interval = 300  # seconds between periodic pool checks (default 5 min)
```

## Cleanup

- **`work nuke`** removes all pool worktrees (and their branches) along with
  regular task worktrees.
- **`work projects delete`** removes pool worktrees for that project before
  deleting the project record.
- Pool entries are also cleaned up via `ON DELETE CASCADE` when a project row is
  deleted from the database.
