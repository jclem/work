#!/usr/bin/env bash
set -euo pipefail

AGENTS_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$AGENTS_DIR/.." && pwd)"

check_mode=false
if [[ "${1:-}" == "--check" ]]; then
  check_mode=true
fi

# Parse vars from config.toml
declare -A vars
while IFS= read -r line; do
  if [[ "$line" =~ ^([a-zA-Z_][a-zA-Z0-9_]*)\ *=\ *\"(.*)\"$ ]]; then
    vars["${BASH_REMATCH[1]}"]="${BASH_REMATCH[2]}"
  fi
done < "$AGENTS_DIR/config.toml"

# Render template: replace {{var}} patterns with values from config
render() {
  local content
  content=$(<"$1")
  for key in "${!vars[@]}"; do
    content="${content//\{\{$key\}\}/${vars[$key]}}"
  done
  printf '%s' "$content"
}

# Sync a rendered file to a destination, or check it matches
sync_file() {
  local src="$1" dest="$2"
  local rendered
  rendered="$(render "$src")"

  if $check_mode; then
    if [[ ! -f "$dest" ]]; then
      echo "MISSING: $dest" >&2
      return 1
    fi
    local existing
    existing=$(<"$dest")
    if [[ "$rendered" != "$existing" ]]; then
      echo "OUT OF SYNC: $dest" >&2
      return 1
    fi
  else
    mkdir -p "$(dirname "$dest")"
    printf '%s' "$rendered" > "$dest"
  fi
}

failed=false

# Sync INSTRUCTIONS.md -> AGENTS.md and CLAUDE.md
for dest_name in AGENTS.md CLAUDE.md; do
  if ! sync_file "$AGENTS_DIR/INSTRUCTIONS.md" "$ROOT_DIR/$dest_name"; then
    failed=true
  fi
done

# Sync skills to .claude/skills/ and .codex/skills/
for skill_dir in "$AGENTS_DIR"/skills/*/; do
  skill_name="$(basename "$skill_dir")"
  src="$skill_dir/SKILL.md"
  [[ -f "$src" ]] || continue

  for target_dir in ".claude/skills" ".codex/skills"; do
    dest="$ROOT_DIR/$target_dir/$skill_name/SKILL.md"
    if ! sync_file "$src" "$dest"; then
      failed=true
    fi
  done
done

if $failed; then
  echo "Sync check failed. Run 'mise run agents:sync' to fix." >&2
  exit 1
fi

if ! $check_mode; then
  echo "Synced agent configs." >&2
fi
