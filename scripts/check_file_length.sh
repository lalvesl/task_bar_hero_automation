#!/usr/bin/env bash
#
# Enforce the per-file line budget clippy cannot: it has no file-level
# counterpart to `too_many_lines`. Test modules are excluded, because a table of
# cases is not the kind of length that hurts a reader.
#
#   ./scripts/check_file_length.sh [max_lines]

set -euo pipefail

max="${1:-350}"
status=0

while IFS= read -r -d '' file; do
  # Count up to the first `#[cfg(test)]`, so test code does not consume the
  # budget meant for the implementation above it.
  lines=$(awk '/^#\[cfg\(test\)\]/ { exit } { n++ } END { print n + 0 }' "$file")
  if (( lines > max )); then
    echo "$file: $lines non-test lines, budget is $max" >&2
    status=1
  fi
done < <(find crates -name '*.rs' -print0)

if (( status == 0 )); then
  echo "file length: all files within $max non-test lines"
fi
exit $status
