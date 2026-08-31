#!/usr/bin/env bash
# Prove an assembled portable build actually works, on a machine that has never
# run the app from source. The Linux/macOS half of scripts/smoke-portable.ps1.
#
# Building an artifact and checking the files are present says nothing about
# whether it runs. This drives the frozen engine the way a user's machine would
# - its own data directory, no developer virtualenv, no Playwright cache - and
# fails loudly on the first thing that does not work:
#
#   1. the engine answers `--version`
#   2. it creates a profile
#   3. it launches the *bundled* Chromium and scores the profile
#      (`trust` opens a real browser; nothing else here proves the bundle)
#   4. `serve` comes up and `/api/ready` answers
#   5. after a restart the profile and its session are still there
#
# Every wait is bounded and polls for a condition. Nothing sleeps blindly.
#
# Usage:  bash scripts/smoke-portable.sh <assembled-folder> [timeout-seconds]
# e.g.    bash scripts/smoke-portable.sh build/linux/Numera

set -euo pipefail

stage="${1:?usage: smoke-portable.sh <assembled-folder> [timeout-seconds]}"
limit="${2:-300}"

deadline=$(( $(date +%s) + limit ))
remaining() {
    local left=$(( deadline - $(date +%s) ))
    if [ "$left" -lt 1 ]; then left=1; fi
    echo "$left"
}
step() { printf '==> %s\n' "$1"; }
fail() { printf 'portable smoke failed: %s\n' "$1" >&2; exit 1; }

engine="$stage/engine/numera-engine"
[ -f "$engine" ] || fail "no frozen engine at $engine"
[ -x "$engine" ] || fail "$engine is not executable - the build lost the exec bit"

# The bundle is the point of the test: without it the engine serves fine and
# fails at the first launch, which is exactly the defect this exists to catch.
# Only the directory is checked here, not a binary by name - the layout inside a
# Chromium build is Playwright's business and has been renamed before
# (chrome-win -> chrome-win64). Step 3 is what proves it can actually launch,
# and Playwright's own "Executable doesn't exist" names the file if it cannot.
browsers="$stage/browsers"
[ -d "$browsers" ] || fail "no bundled browser at $browsers"
ls -d "$browsers"/chromium-* >/dev/null 2>&1 \
    || fail "$browsers has no chromium-* build - the portable folder cannot launch a browser"
step "Bundled browser: $(ls -d "$browsers"/chromium-* | head -n 1)"

# A data directory of its own, and no inherited Playwright cache: this has to
# pass the way it would on a machine that has never seen this project.
data="$(mktemp -d "${TMPDIR:-/tmp}/numera-smoke.XXXXXX")"
unset PLAYWRIGHT_BROWSERS_PATH
serve_pid=""
cleanup() {
    if [ -n "$serve_pid" ]; then kill -9 "$serve_pid" 2>/dev/null || true; fi
    rm -rf "$data" "$data.log" "$data.serve.log"
}
trap cleanup EXIT

# Run one engine command with a deadline, leaving its exit code in $status and
# its output in $output. macOS ships no `timeout`, so the wait polls the child
# rather than depending on coreutils being installed.
#
# A non-zero exit aborts the run unless the caller sets allow_failure=1 first;
# that flag is one-shot so it cannot leak into the next command.
allow_failure=0
status=0
output=""
run_engine() {
    local seconds="$1"; shift
    # Never past the run's own deadline: without this clamp the per-command
    # ceilings are the only real bound and the script can outlive the budget it
    # was given several times over.
    local left; left="$(remaining)"
    if [ "$seconds" -gt "$left" ]; then seconds="$left"; fi
    "$engine" --data-dir "$data" "$@" >"$data.log" 2>&1 &
    local pid=$!
    local start=$SECONDS
    while kill -0 "$pid" 2>/dev/null; do
        if [ $(( SECONDS - start )) -ge "$seconds" ]; then
            kill -9 "$pid" 2>/dev/null || true
            fail "\`$*\` did not finish within ${seconds}s: $(cat "$data.log")"
        fi
        sleep 0.5
    done
    if wait "$pid"; then status=0; else status=$?; fi
    output="$(cat "$data.log")"
    if [ "$status" -ne 0 ] && [ "$allow_failure" -eq 0 ]; then
        fail "\`$*\` exited $status: $output"
    fi
    allow_failure=0
}

step '1/5 the engine runs at all'
run_engine 30 --version
[ -n "$output" ] || fail '--version printed nothing'
printf '    %s\n' "$output"

step '2/5 it creates a profile'
run_engine 60 create smoke
run_engine 60 list
grep -q smoke <<<"$output" || fail "the profile is not in \`list\`: $output"

step '3/5 it launches the bundled browser'
# `trust` opens a real browser and reads back what the page shows. Nothing short
# of launching proves the bundle resolves.
# A non-zero exit here means the *profile* scored imperfectly, which is not what
# this test is about: the question is whether the bundled browser opened at all,
# and a report with real checks in it only exists if it did.
allow_failure=1
run_engine "$(remaining)" trust smoke
grep -qi 'trust score' <<<"$output" \
    || fail "\`trust\` produced no report - the browser never opened: $output"
grep -q '\[PASS\]' <<<"$output" \
    || fail "\`trust\` reported no passing check - the page was never inspected: $output"
grep -m1 -i 'trust score' <<<"$output"

step '4/5 serve comes up and answers /api/ready'
# Ask the kernel for a free port rather than hoping a fixed one is idle: two
# jobs on one runner would otherwise collide and read as a flaky build.
port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
"$engine" --data-dir "$data" serve --port "$port" >"$data.serve.log" 2>&1 &
serve_pid=$!
ready=""
until=$(( $(date +%s) + 90 ))
while [ "$(date +%s)" -lt "$until" ] && [ "$(remaining)" -gt 1 ]; do
    kill -0 "$serve_pid" 2>/dev/null \
        || fail "serve exited before answering: $(cat "$data.serve.log")"
    ready="$(curl -sf --max-time 3 "http://127.0.0.1:$port/api/ready" || true)"
    if [ -n "$ready" ]; then break; fi
    sleep 0.25   # bounded poll, not a blind wait
done
[ -n "$ready" ] || fail "/api/ready never answered on port $port: $(cat "$data.serve.log")"
grep -q '"ok": *true' <<<"$ready" || fail "/api/ready answered but not ok: $ready"
printf '    %s\n' "$ready"

step '5/5 the profile and its session survive a restart'
kill "$serve_pid" 2>/dev/null || true
for _ in $(seq 1 40); do
    kill -0 "$serve_pid" 2>/dev/null || break
    sleep 0.25
done
kill -9 "$serve_pid" 2>/dev/null || true
wait "$serve_pid" 2>/dev/null || true
serve_pid=""
run_engine 60 list
grep -q smoke <<<"$output" || fail "the profile did not survive the restart: $output"
ls -d "$data"/user-data/*/ >/dev/null 2>&1 || fail 'the launch left no session directory behind'

printf '\nPORTABLE SMOKE: PASS\n'
