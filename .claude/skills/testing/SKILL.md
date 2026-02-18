---
name: testing
description: Guidelines for writing and maintaining tests. Use when adding tests, modifying testable logic, or validating CLI behavior.
---

# Testing

Follow these guidelines when writing or updating tests.

## Running tests

- Default validation entrypoint: `mise run check`.
- Focused test command: `mise run test` (or `cargo test`).
- Formatting/lint checks: `mise run check:fmt` and `mise run check:clippy`.

## Unit tests

- Place unit tests in `#[cfg(test)] mod tests` blocks in the relevant module file.
- Prefer testing pure logic, parsing, error mapping, and deterministic output behaviors.
- Keep tests self-contained and avoid hidden fixture coupling.

## CLI behavior tests

- Test argument parsing and clap constraints (`requires`, `conflicts_with`, invalid combinations).
- Test help output shape for changed commands.
- Verify both exit code and output content for failure paths.

## Conventions

- Name tests as `<thing_under_test>_<scenario>`.
- Use `assert_eq!` for concrete values and `matches!` for enum variants.
- For textual errors, assert on stable substrings instead of exact full strings.
- Keep `#[cfg(test)] mod tests` as the last item in a file when practical.

## Checklist

1. New logic has tests.
2. New command flags/subcommands have parse and behavior coverage.
3. `mise run test` passes.
4. `mise run check:clippy` and `mise run check:fmt` pass.