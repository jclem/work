#!/bin/sh
set -eu

# Run a specific test, or all test_*.sh scripts.
if [ $# -gt 0 ]; then
    exec "$@"
fi

failed=0
for f in /build/tests/docker/test_*.sh; do
    [ -f "$f" ] || continue
    name=$(basename "$f")
    echo "==> $name"
    if sh "$f"; then
        echo ""
    else
        failed=1
        echo ""
    fi
done

if [ "$failed" -ne 0 ]; then
    exit 1
fi
