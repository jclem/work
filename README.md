# work

To run:

```bash
cargo run -- daemon start
```

To check:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## CLI

Start the daemon:

```bash
work daemon start
```

Or run with cargo:

```bash
cargo run -- daemon start
```

Print the resolved daemon socket path (for scripting):

```bash
work daemon socket-path
```

Create a project (defaults to current directory basename as name):

```bash
work projects create
```

Create a project for a specific path:

```bash
work projects create /path/to/project
```

Override the project name:

```bash
work projects create /path/to/project --name my-project
```

List projects:

```bash
work projects list
work projects ls
work ls
work projects list --plain
work projects list --json
```

Delete a project:

```bash
work projects delete my-project
```

Enable shell completions:

```bash
eval "$(work completions zsh)"
```

You can also generate other shells:

```bash
work completions bash
work completions fish
```

The daemon listens for HTTP over a unix domain socket at:
- `$XDG_RUNTIME_DIR/workd/workd.sock` when `XDG_RUNTIME_DIR` is set
- otherwise a temp-dir fallback from the platform (for example `/tmp/workd/workd.sock`)

You can override the socket path with:

```bash
work daemon start --socket /path/to/workd.sock
```

Output conventions:
- machine-readable values (for example `daemon socket-path`) print to `stdout`
- status, warnings, and errors print to `stderr`
