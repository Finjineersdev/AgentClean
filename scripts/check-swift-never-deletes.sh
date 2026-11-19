#!/bin/sh
# ADR 0002: only the Rust engine moves or deletes user files. Fails if app or
# library Swift code calls a file removal or move API. Test code may remove
# the temporary folders it created, so test folders are not checked.
set -eu
cd "$(dirname "$0")/.."
pattern='removeItem|trashItem|moveItem|replaceItem|replaceItemAt|unlink[[:space:]]*\(|rmdir[[:space:]]*\(|removefile|copyfile|[^a-zA-Z_.]rename[[:space:]]*\(|renameat|NSWorkspace.*recycle|"rm"|/bin/rm|"mv"|/bin/mv'
if grep -rnE "$pattern" --include='*.swift' --include='*.c' --include='*.m' AgentClean CleanCore/Sources; then
  echo "error: Swift code must not move or delete files; ask the engine (ADR 0002)." >&2
  exit 1
fi
echo "ok: no file removal or move calls in the app or CleanCore"
