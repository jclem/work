#!/bin/sh
# Test that project detection works from worktree directories
# and that `work cd` navigates to the project root when no task is given.
. /build/tests/docker/harness.sh

setup_git
PROJ=$(setup_project)
cd "$PROJ"

# Create a task
work new test-task --no-cd 2>/dev/null
TASK_PATH=$(work list --plain | awk '{print $2}')

# Sanity check
if [ -z "$TASK_PATH" ]; then
    echo "FATAL: task was not created" >&2
    exit 1
fi

# -- project detection from worktree paths --------------------------------

cd "$TASK_PATH"
assert_ok "list from worktree dir" work list

mkdir -p "$TASK_PATH/sub/dir"
cd "$TASK_PATH/sub/dir"
assert_ok "list from worktree subdirectory" work list

cd "$TASK_PATH"
work new second-task --no-cd 2>/dev/null
LIST=$(work list --plain)
assert_eq "$(echo "$LIST" | wc -l | tr -d ' ')" "2" "new + list from worktree shows both tasks"
work delete second-task --force 2>/dev/null

# -- work cd (no task name) -----------------------------------------------

cd "$PROJ"
EVAL=$(work_eval cd)
assert_contains "$EVAL" "$PROJ" "cd from project dir -> project root"

cd "$TASK_PATH"
EVAL=$(work_eval cd)
assert_contains "$EVAL" "$PROJ" "cd from worktree dir -> project root"

cd /tmp
EVAL=$(work_eval cd --project test-project)
assert_contains "$EVAL" "$PROJ" "cd --project from unrelated dir -> project root"

# -- work cd <task> from worktree ----------------------------------------

cd "$TASK_PATH"
EVAL=$(work_eval cd test-task)
assert_contains "$EVAL" "$TASK_PATH" "cd <task> from worktree dir -> task path"

summary
