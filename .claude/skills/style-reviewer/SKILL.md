---
name: style-reviewer
description: Review Rust CLI code for style-guide conformance and idiomatic Rust/clap design. Use when asked to assess or enforce CLI style, error handling quality, or UX consistency.
---

# Style Reviewer

Perform a style review for this repository's Rust CLI.

Prioritize:
- Conformance with `AGENTS.md` (`CLI style guide` and `Rust style guide` sections).
- Idiomatic Rust and `clap` best practices.
- Clear, source-aware error handling and output.
- Consistent, developer-friendly CLI UX.

When project style guidance and generic best practice differ, prefer project guidance.

## Review workflow

1. Load context:
- Read `AGENTS.md`.
- Read relevant CLI code (`src/main.rs`, `src/commands/**/*.rs`, plus supporting modules such as `src/cli.rs`, `src/error.rs`, `src/db.rs`, and `src/workd.rs`).
2. Run verification commands:
- `mise run check:fmt`
- `mise run check:clippy`
- `cargo run -- --help`
- `cargo run -- --version`
- `cargo run -- <subcommand> --help` for changed/important subcommands
3. Evaluate:
- clap design and argument relationships
- Error typing and output consistency
- Exit code semantics
- UX clarity in help and status output
4. Present findings:
- Scope and commands run
- Conformance summary
- Concrete deviations with file references

## Output rules

- Ground findings in code and observed behavior, not preference.
- Keep recommendations aligned to repository conventions.
- Use file references for every concrete issue.
- Do not apply fixes unless explicitly requested.