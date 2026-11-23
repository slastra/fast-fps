#!/bin/bash
# Run every check this project has. Use it before calling a change done.
#
#   scripts/check.sh           format, lint, unit tests, then every harness scenario
#   scripts/check.sh --quick   format, lint, unit tests, and the smoke scenario only
#
# Harness scenarios are listed in tests/harness/suite.txt. Exit status is 0
# only if everything passed. A full run takes a few minutes.
set -u
cd "$(dirname "$0")/.." || exit 2

QUICK=0
[ "${1:-}" = --quick ] && QUICK=1

RESULTS=()
failed=0
step() {
    local label=$1
    shift
    echo
    echo "=== $label"
    if "$@"; then
        RESULTS+=("PASS  $label")
    else
        RESULTS+=("FAIL  $label")
        failed=1
    fi
}

step "format (cargo fmt --check)" cargo fmt --all -- --check
step "lint (cargo clippy)" cargo clippy --all-targets -- -D warnings
step "unit tests (cargo test)" cargo test

if [ $QUICK = 1 ]; then
    step "harness: smoke" scripts/harness.sh tests/harness/smoke.cfg
else
    while IFS= read -r line; do
        case $line in '' | '#'*) continue ;; esac
        # shellcheck disable=SC2086  # the line is a list of arguments
        step "harness: $line" scripts/harness.sh $line
    done <tests/harness/suite.txt
fi

echo
echo "=== summary"
printf '%s\n' "${RESULTS[@]}"
if [ $failed = 0 ]; then echo "ALL PASSED"; else echo "SOMETHING FAILED"; fi
exit $failed
