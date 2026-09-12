#!/bin/sh
# Refuse absolute paths from a contributor's own machine.
#
# A `file:///Users/alice/...` link in a README is broken for every reader and
# publishes the author's home directory. This is how one got committed, so the
# check runs with the rest of `make check` rather than relying on review.
#
# Tracked files only: a local build directory is not our business.
set -eu

repository_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repository_root"

# A home directory under any of the three platform layouts. `file://` on its own
# is fine — the install test serves a fake release from a temp directory that
# way — so only a named home is refused.
pattern='/Users/[A-Za-z0-9._-]+/|/home/[A-Za-z0-9._-]+/|C:\\Users\\[A-Za-z0-9._-]+\\'

# The check describes the very thing it forbids, so it would match itself.
matches="$(git ls-files -z \
    | grep -zv '^scripts/check-no-local-paths\.sh$' \
    | xargs -0 grep -nEI "$pattern" 2>/dev/null || true)"

if [ -n "$matches" ]; then
    echo "error: a path from someone's own machine is committed:" >&2
    echo "$matches" >&2
    echo >&2
    echo "Use a repository-relative link, or ~ for a home directory." >&2
    exit 1
fi

echo "no local absolute paths in tracked files"
