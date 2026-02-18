---
name: commands
description: Guidelines for adding or updating CLI commands and their inputs/outputs. Use when creating new subcommands, changing argument shapes, or modifying command output.
---

# Commands

Follow these guidelines when adding or modifying CLI commands.

## Error output

- Format errors as multi-line output: bold red `error:` prefix, then the message. If a hint is available, print it on the next line with a cyan bold `hint:` label and dimmed hint text.
- Attach a `hint: Option<String>` to error variants where a concrete next step can be suggested.
- Hide the source chain by default. Only print `caused by:` lines when the global `--verbose` flag is set.
- Use shared error output helpers in `src/error.rs` for consistency.

## Success output

- Confirm what happened: avoid silent success for operations that otherwise produce no output.
- Use shared success output helpers in `src/error.rs` so symbols and colors are centralized.

## Exit codes

- Keep exit code mappings centralized in `src/error.rs`.
- `2` is reserved for clap usage errors.
- When adding new error variants, assign and document exit codes intentionally.

## Command naming and shape

- Use `clap` derive style (`Subcommand` enum variant + `Args` struct).
- Put doc comments on enum variants for `--help` text.
- Use positional args for the single primary operand.
- Use named flags for optional modifiers and secondary options.
- Express argument relationships in `clap` attributes (`requires`, `conflicts_with`) instead of runtime checks.
- Hide internal/setup-oriented subcommands with `#[command(hide = true)]` when that improves the public CLI UX.

## Checklist

1. `mise run check` passes.
2. Error paths produce actionable output with correct exit codes.
3. Success paths confirm completion for otherwise silent operations.
4. Help text remains concise and accurate.