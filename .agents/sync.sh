#!/usr/bin/env bash
set -euo pipefail

AGENTS_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$AGENTS_DIR/.." && pwd)"

check_mode=false
if [[ "${1:-}" == "--check" ]]; then
  check_mode=true
fi

# Parse vars from config.toml (supports nested sections like [vars.coauthor])
declare -A vars
in_vars=false
current_prefix=""
while IFS= read -r line; do
  if [[ "$line" =~ ^\[vars\]$ ]]; then
    in_vars=true
    current_prefix=""
  elif [[ "$line" =~ ^\[vars\.([a-zA-Z_][a-zA-Z0-9_.]*)\]$ ]]; then
    in_vars=true
    current_prefix="${BASH_REMATCH[1]}."
  elif [[ "$line" =~ ^\[.*\]$ ]]; then
    in_vars=false
  elif $in_vars && [[ "$line" =~ ^([a-zA-Z_][a-zA-Z0-9_]*)\ *=\ *\"(.*)\"$ ]]; then
    vars["${current_prefix}${BASH_REMATCH[1]}"]="${BASH_REMATCH[2]}"
  fi
done < "$AGENTS_DIR/config.toml"

# Render template: replace {{var}} patterns with values from config.
# When an agent name is provided, agent-specific vars take priority.
# E.g. with agent "claude", {{coauthor}} resolves to vars["coauthor.claude"].
render() {
  local content agent="${2:-}"
  content=$(<"$1")

  if [[ -n "$agent" ]]; then
    for key in "${!vars[@]}"; do
      if [[ "$key" == *".$agent" ]]; then
        local base_key="${key%."$agent"}"
        content="${content//\{\{$base_key\}\}/${vars[$key]}}"
      fi
    done
  fi

  for key in "${!vars[@]}"; do
    content="${content//\{\{$key\}\}/${vars[$key]}}"
  done
  printf '%s' "$content"
}

# Sync a rendered file to a destination, or check it matches
sync_file() {
  local src="$1" dest="$2" agent="${3:-}"
  local rendered
  rendered="$(render "$src" "$agent")"

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

  for target in claude codex; do
    dest="$ROOT_DIR/.$target/skills/$skill_name/SKILL.md"
    if ! sync_file "$src" "$dest" "$target"; then
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
