# AGENTS.md instructions for /Users/jclem/src/github.com/jclem/work

## Runtime defaults

- Default to Rust for all implementation tasks.
- Use `cargo` for dependency management and Rust build/test primitives.
- Do not use Bun/Node/Bun APIs unless the request is explicitly non-Rust and unrelated.

## Commands

- Prefer `mise` task aliases from `mise.toml` when available for environment and command orchestration.
- Use `mise run test` (or `cargo test`) as the default test command.
- Use `mise run check:clippy` (or `cargo clippy --all-targets --all-features -- -D warnings`) for linting.
- Use `mise run check:fmt` (or `cargo fmt --check`) for formatting verification.
- Use `mise run dev` for daemon startup during development.
- Use `cargo run -- <subcommand>` for general ad-hoc CLI execution.
- Use `mise run check` to run all checks.
- Use `mise run fix` for auto-fixing and formatting/lint cleanup.
- Use `mise run release:local` for release build + local install.
- Use `mise run pre-commit` as the default pre-commit validation entrypoint.

Example common aliases in this repo are usually:

```sh
mise run check
mise run test
mise run check:fmt
mise run check:clippy
mise run dev
```

## APIs

- Prefer Rust stdlib and idiomatic crates already in the workspace dependencies.
- Use the repo's actual stack:
- `tokio` + `axum` for async networking/server paths.
- `rusqlite` for SQLite persistence.
- `clap`/`clap_complete` for CLI parsing and shell completions.
- `serde`/`serde_json` for serialization.
- Avoid introducing new framework crates when existing dependencies already cover the use case.
- Use compile-time safety and typed APIs over dynamic/runtime tricks when possible.

## CLI style guide

- Format errors as multi-line output: bold red `error:` prefix on the first line, optional cyan `hint:` with dimmed hint text on a second indented line. Keep default output short and actionable.
- Use progressive disclosure for error details: hide source chains by default and show them only with `--verbose` (`caused by:` lines, dimmed).
- Attach structured hints (`hint: Option<String>`) to error variants where a concrete fix can be suggested.
- Use meaningful exit codes so scripts can distinguish failure modes:
- `1` = generic/internal errors.
- `2` = usage errors (clap parse failures).
- Define and document any additional non-zero codes in `src/error.rs` when adding new classes of failures.
- Confirm what happened: print a green checkmark success message to stderr for operations that would otherwise complete silently.
- Use shared `print_success` and `print_error` utilities in `src/error.rs` so formatting stays centralized.
- Provide shell completions with a dedicated subcommand when completions are supported.

## Rust style guide

- Use `clap` derive style for CLI definitions (`Parser`, `Subcommand`, `Args`) with command and arg attributes colocated on the relevant type or field.
- Define root CLI metadata directly on the parser (`name`, `version`, `about`, `long_about = None`, `propagate_version = true`).
- Use enum variant doc comments for subcommand help text.
- Use positional arguments for the single primary operand of a command. Use named flags for optional modifiers and secondary options.
- Express argument relationships in `clap` attributes (`requires`, `conflicts_with`) so parse-time UX is explicit.
- Represent finite CLI values with `clap::ValueEnum`, and use typed defaults with `default_value_t`.
- Model errors with structured variants that preserve source context for IO/network/parse paths and message-style variants for user-facing outcomes.
- Print errors to stderr. Include source-chain detail only when verbose output is requested.
- Apply color to styled output only when stderr is a terminal and `NO_COLOR` is not set.
- Keep code formatting and linting aligned with `mise run check:fmt` and `mise run check:clippy`.

## Testing

Use Rust test modules and `mise run test` (or `cargo test`).

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works() {
        assert_eq!(2 + 2, 4);
    }
}
```

## Project structure

- Keep Rust entrypoints minimal and composable.
- Use `src/main.rs` for application bootstrapping.
- Keep clap parser and command shape definitions in `src/cli.rs`.
- Keep command implementations under `src/commands/` (for example `src/commands/projects.rs`, `src/commands/daemon.rs`).
- Keep reusable concerns in focused modules under `src/` (for example `src/db.rs`, `src/workd.rs`, `src/paths.rs`, `src/logger.rs`, `src/error.rs`).
- Keep new modules small and focused.
- Favor explicit error types (e.g. `anyhow` or typed enums) and avoid swallowing errors.
- Add docs/comments for non-obvious behavior and public interfaces.

## Frontend considerations (when applicable)

- If the repo includes frontend code, keep it in a Rust-compatible build flow (e.g., framework-specific tooling already in use).
- If generating HTML/JS assets, treat the Rust app execution and build as source of truth.
