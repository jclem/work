#!/bin/sh
# Test harness for Docker integration tests.
# Source this at the top of every test script.

_FAILURES=0
_PASSES=0

pass() { _PASSES=$((_PASSES + 1)); printf '\033[32m  PASS\033[0m %s\n' "$1"; }
fail() { _FAILURES=$((_FAILURES + 1)); printf '\033[31m  FAIL\033[0m %s\n' "$1"; }

assert_contains() {
    if echo "$1" | grep -qF "$2"; then
        pass "$3"
    else
        fail "$3 (expected to contain '$2', got '$1')"
    fi
}

assert_eq() {
    if [ "$1" = "$2" ]; then
        pass "$3"
    else
        fail "$3 (expected '$2', got '$1')"
    fi
}

assert_ok() {
    local label="$1"; shift
    if "$@" >/dev/null 2>&1; then
        pass "$label"
    else
        fail "$label (command failed: $*)"
    fi
}

assert_fail() {
    local label="$1"; shift
    if "$@" >/dev/null 2>&1; then
        fail "$label (expected failure: $*)"
    else
        pass "$label"
    fi
}

# Run a work command that writes to WORK_SHELL_EVAL and return the output.
work_eval() {
    local f; f=$(mktemp)
    WORK_SHELL_EVAL="$f" work "$@" 2>/dev/null || true
    cat "$f"
    rm -f "$f"
}

setup_git() {
    git config --global user.email "test@test.com"
    git config --global user.name "Test"
    git config --global init.defaultBranch main
}

# Create a git repo, start the daemon, register the project.
# Usage: PROJ=$(setup_project [DIR])
setup_project() {
    local dir="${1:-/tmp/test-project}"
    mkdir -p "$dir"
    git -C "$dir" init --quiet
    echo "test" > "$dir/README.md"
    git -C "$dir" add . && git -C "$dir" commit -m "init" --quiet
    work daemon start 2>/dev/null
    sleep 1
    work projects create "$dir" 2>/dev/null
    printf '%s' "$dir"
}

summary() {
    echo ""
    work daemon stop 2>/dev/null || true
    if [ "$_FAILURES" -eq 0 ]; then
        printf '\033[32mAll %d test(s) passed.\033[0m\n' "$_PASSES"
    else
        printf '\033[31m%d failed, %d passed.\033[0m\n' "$_FAILURES" "$_PASSES"
        exit 1
    fi
}
