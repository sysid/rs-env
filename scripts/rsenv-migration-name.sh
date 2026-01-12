#!/usr/bin/env bash
set -euo pipefail

# ROOT="${1:-.}"
ROOT="${RSENV_VAULT:-.}"

echo "-M- working on $ROOT"

find "$ROOT" -depth -name '*rsenv_active' | while IFS= read -r path; do
  base="$(basename "$path")"
  dir="$(dirname "$path")"

  # Match: <prefix>.<id>.rsenv_active
  # - prefix may start with '.' and may contain other chars except '/'
  # - id is the middle token (no dots)
  if [[ "$base" =~ ^(.+)\.([^.]+)\.rsenv_active$ ]]; then
    prefix="${BASH_REMATCH[1]}"
    id="${BASH_REMATCH[2]}"
    new="${prefix}@@${id}@@rsenv_active"

    if [[ "$base" != "$new" ]]; then
      mv -v -- "$path" "$dir/$new"
    fi
  fi
done
