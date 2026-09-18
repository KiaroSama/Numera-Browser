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
#   5. `/` returns the real dashboard, and an asset that page references is
#      served (the API alone is not the product)
#   6. after a restart the profile and its session are still there
#
# Every wait is bounded and polls for a condition. Nothing sleeps blindly.
#
# Usage:  bash scripts/smoke-portable.sh <assembled-folder> [timeout-seconds]
# e.g.    bash scripts/smoke-portable.sh 'build/linux/Numera Browser'

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
stock_browsers="$browsers"
if [ -d "$browsers/playwright" ]; then stock_browsers="$browsers/playwright"; fi
ls -d "$stock_browsers"/chromium-* >/dev/null 2>&1 \
    || fail "$browsers has no chromium-* build - the portable folder cannot launch a browser"
step "Bundled browser: $(ls -d "$stock_browsers"/chromium-* | head -n 1)"

# A data directory of its own, and no inherited Playwright cache: this has to
# pass the way it would on a machine that has never seen this project.
data="$(mktemp -d "${TMPDIR:-/tmp}/numera-smoke.XXXXXX")"
# Migration has its own tests; the smoke fixture must not copy personal data.
printf '%s\n' 'Smoke fixture: skip legacy import.' > "$data/.imported"
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

step '1/9 the engine runs at all'
run_engine 30 --version
[ -n "$output" ] || fail '--version printed nothing'
printf '    %s\n' "$output"

step '2/9 it creates a profile'
run_engine 60 create smoke
run_engine 60 list
grep -q smoke <<<"$output" || fail "the profile is not in \`list\`: $output"

step '3/9 it launches the bundled browser'
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

step '4/9 serve comes up and answers /api/ready'
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

step '5/9 the dashboard is served, not just the API'
# `/api/ready` says nothing about the UI. The engine serves the dashboard out of
# `numera/web`, and a freeze that collects the Python modules but forgets that
# directory answers every API route while `/` 404s - an artifact that passes a
# file listing, uploads happily, and opens to nothing. That was the state of the
# Linux and macOS builds until --add-data was added to the freeze.
#
# The asset is the second half of the check. index.html is one file and can
# arrive on its own; the hashed bundle it points at only resolves if the whole
# directory came across, and without that bundle the page is a blank <div>.
html="$(curl -sf --max-time 5 "http://127.0.0.1:$port/" || true)"
grep -q '<div id="root">' <<<"$html" \
    || fail "/ did not return the dashboard: ${html:-<no response>}"
asset="$(grep -o '/assets/[^"]*' <<<"$html" | head -n 1 || true)"
[ -n "$asset" ] || fail "the dashboard page references no /assets/ file: $html"
curl -sf --max-time 5 -o /dev/null "http://127.0.0.1:$port$asset" \
    || fail "the page references $asset and the build does not serve it"
printf '    dashboard served, %s resolves\n' "$asset"

step '6/9 the API launches and stops a browser - the frozen self-spawn path'
# The one path a packaged build has that a source checkout does not. Step 3
# proved the bundle launches from the CLI; this proves the *server* can do it,
# and the only difference between them is that the frozen engine has to
# re-execute itself. That is exactly where the PyInstaller entry point broke
# before - `numera/__main__.py` used a relative import, so the artifact died on
# `--version` - and nothing in this script would have noticed, because the CLI
# path does not self-spawn.
api() {
    local method="$1" path="$2"
    curl -sf --max-time 60 -X "$method" "http://127.0.0.1:$port$path" || true
}

pid_json="$(curl -sf --max-time 10 "http://127.0.0.1:$port/api/profiles" || true)"
prof_id="$(python3 -c "
import json, sys
rows = json.loads(sys.argv[1] or '[]')
print(rows[0]['id'] if rows else '')
" "$pid_json")"
[ -n "$prof_id" ] || fail "the API lists no profile to launch: ${pid_json:-<none>}"

launched="$(api POST "/api/profiles/$prof_id/launch")"
grep -q '"ok" *: *true\|"status"' <<<"$launched"     || fail "the API launch did not answer with a result: ${launched:-<none>}"

# Ownership, from the product's own surface rather than a guess: the profile
# has to read back as running, and its session directory has to exist.
running=""
until=$(( $(date +%s) + 60 ))
while [ "$(date +%s)" -lt "$until" ] && [ "$(remaining)" -gt 1 ]; do
    running="$(curl -sf --max-time 5 "http://127.0.0.1:$port/api/profiles" || true)"
    if grep -q '"status" *: *"running"' <<<"$running"; then break; fi
    sleep 0.25   # bounded poll
done
grep -q '"status" *: *"running"' <<<"$running"     || fail "the API launched a browser and the profile never read back as running"
ls -d "$data"/user-data/"$prof_id"/ >/dev/null 2>&1     || fail "the API launch created no session directory for $prof_id"

stopped="$(api POST "/api/profiles/$prof_id/stop")"
[ -n "$stopped" ] || fail "the API stop returned nothing"
idle=""
until=$(( $(date +%s) + 60 ))
while [ "$(date +%s)" -lt "$until" ] && [ "$(remaining)" -gt 1 ]; do
    idle="$(curl -sf --max-time 5 "http://127.0.0.1:$port/api/profiles" || true)"
    if ! grep -q '"status" *: *"running"' <<<"$idle"; then break; fi
    sleep 0.25
done
grep -q '"status" *: *"running"' <<<"$idle"     && fail "the API stop returned and the profile is still running - the tree outlived it"
printf '    launched and stopped through the API, session %s
' "$prof_id"

step '7/9 a finite helper runs to completion through the API'
# `run_helper` in the frozen artifact. A probe starts its own browser, writes an
# ownership report and re-executes this binary to do it - the same self-spawn as
# above, on the finite path. Its answer is only usable if the stream was read to
# the end, and the failure this guards against is a *complete-looking* reply
# that stops halfway: asserting the body parses and carries the report's own
# keys is the stream assertion, because half a JSON document does not.
probe="$(curl -sf --max-time 240 -X POST          "http://127.0.0.1:$port/api/profiles/$prof_id/trust" || true)"
[ -n "$probe" ] || fail "the API trust probe returned nothing - the helper never answered"
python3 -c "
import json, sys
body = sys.argv[1]
try:
    data = json.loads(body)
except ValueError as e:
    raise SystemExit(f'the helper answer is not whole JSON ({e}): {body[:400]}')
if not isinstance(data, dict):
    raise SystemExit(f'the helper answered {type(data).__name__}, not a report')
# The two keys the route itself reads back off the report. If the stream was
# cut, the body either fails to parse or arrives without them.
for key in ('grade', 'score'):
    if key not in data:
        raise SystemExit(f'the helper report has no {key!r}: {body[:400]}')
" "$probe" || fail 'the finite helper did not return a complete answer'

# Nothing it owned may outlive it. `/api/ready` does not report stragglers, so
# the assertion is made against what the product does expose: the probe opened a
# browser, and once it has answered nothing may still be running.
after="$(curl -sf --max-time 10 "http://127.0.0.1:$port/api/profiles" || true)"
grep -q '"status" *: *"running"' <<<"$after"     && fail "the finite helper answered and left a browser running: $after"
printf '    finite helper answered whole, nothing left running
'

step '8/9 the profile and its session survive a restart'
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

step '9/9 the vault encrypts a real secret and reopens it'
# `cryptography` lives in the `secure` extra, and a frozen app cannot
# pip-install into its own embedded runtime - so a build without that extra
# fails here and nowhere earlier. The product advertises an encrypted vault;
# this is what proves the shipped artifact has one.
secret='smoke-proxy-password'
printf 'http://u:%s@203.0.113.7:8080\n' "$secret" > "$data/smoke-proxies.txt"
run_engine 60 pool add "$data/smoke-proxies.txt"
grep -q "$secret" "$data/proxies.json" \
    || fail 'the proxy password is not readable in the pool file before enabling the vault'

run_engine 120 --password smoke-master vault enable

grep -q "$secret" "$data/proxies.json" \
    && fail 'vault enable reported success but the password is still readable on disk'
grep -qE 'enc:v[0-9]+:' "$data/proxies.json" \
    || fail 'the stored secret is neither plaintext nor an encrypted envelope'

# Encrypting is only half of it - a vault you cannot unlock is data loss.
run_engine 60 --password smoke-master pool export
grep -q "$secret" <<<"$output" \
    || fail 'the vault would not give the secret back with the master password'
allow_failure=1
run_engine 60 pool export
grep -q "$secret" <<<"$output" && fail 'a locked export handed out the secret'
printf '    encrypted at rest, readable with the master password, withheld without it\n'

printf '\nPORTABLE SMOKE: PASS\n'
