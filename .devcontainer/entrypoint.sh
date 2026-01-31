#!/usr/bin/env bash
set -euo pipefail

# ---------------------------------------------------------------------------
# Best-effort ownership fix for /workspace
#
# When a volume is mounted into the container the files keep their host
# ownership.  If we started as root (common for the very first `docker
# compose up` without a --user flag) fix ownership so that the devuser
# can read and write without friction.
#
# If we are already running as a non-root user that simply doesn't own the
# files there is nothing we can do — the user should either:
#   1. Run once as root:  docker compose run --rm --user root dev chown -R devuser:devuser /workspace
#   2. Or start with --user matching the host UID:  docker compose up
#      (compose.yml already passes `user: "${UID}:${GID}"`)
# ---------------------------------------------------------------------------
if [ "$(id -u)" -eq 0 ] && [ -d /workspace ]; then
    # Current owner UID of /workspace
    ws_uid="$(stat -c '%u' /workspace 2>/dev/null || echo 0)"

    if [ "$ws_uid" -ne 1000 ]; then
        chown -R devuser:devuser /workspace
    fi
fi

exec "$@"
