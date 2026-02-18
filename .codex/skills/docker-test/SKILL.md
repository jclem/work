---
name: docker-test
description: Run integration tests in an Alpine Docker container. Use when asked to test CLI behavior in Docker or verify cross-platform compatibility.
user_invocable: true
---

# Docker Integration Tests

Run CLI integration tests in an isolated Alpine Linux container.

## Running tests

```sh
mise run test:docker
```

The Docker build caches compiled dependencies in a separate layer, so only the
first run is slow. Subsequent runs recompile only the project binary.

## Writing tests

Place test scripts in `tests/docker/` with a `test_` prefix
(e.g., `tests/docker/test_worktree_detection.sh`). The entrypoint runs every
matching script automatically.

```sh
#!/bin/sh
. /build/tests/docker/harness.sh

setup_git
PROJ=$(setup_project)

# ... assertions ...

summary
```

## Harness API

### Setup / teardown

| Function | Description |
|---|---|
| `setup_git` | Configure git identity and default branch |
| `setup_project [DIR]` | Init a repo, start the daemon, register the project. Prints project path. Default: `/tmp/test-project` |
| `summary` | Print results, stop daemon, exit non-zero on failure. **Must** be the last call. |

### Assertions

| Function | Description |
|---|---|
| `pass LABEL` | Record a passing assertion |
| `fail LABEL` | Record a failing assertion |
| `assert_contains ACTUAL EXPECTED LABEL` | Assert `ACTUAL` contains substring `EXPECTED` |
| `assert_eq ACTUAL EXPECTED LABEL` | Assert string equality |
| `assert_ok LABEL CMD...` | Assert command exits 0 |
| `assert_fail LABEL CMD...` | Assert command exits non-zero |

### Utilities

| Function | Description |
|---|---|
| `work_eval [ARGS...]` | Run `work` with a temporary `WORK_SHELL_EVAL` file and print what was written |

## Checklist

1. Test script has the `test_` prefix and is in `tests/docker/`.
2. Script sources the harness and calls `summary` at the end.
3. `mise run test:docker` passes.