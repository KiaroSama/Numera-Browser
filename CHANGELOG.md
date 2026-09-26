# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

Three reviewed correctness fixes went in after 0.36.0, and the round of repairs
that reviewing them found. The shape they share: a check that read a label
instead of the thing the label described.

- Choose a profile's validation context by the value the caller declared, and
  refuse a declared container whose contents do not match that context as a
  domain error rather than routing it to the validator that permits the gap.
- Validate the **active persisted** vault descriptor on every recovery path,
  and read back what actually reached `config.json` instead of trusting the
  write that claimed to put it there.
- End a helper's processes and streams under one deadline, and when a straggler
  cannot be accounted for, retain it and say so by name - including when a
  bounded settle gives up because another caller held the lock for the whole
  budget.
- Say which category a rejected ownership record fell into as well as what was
  wrong with it, so a caller matching on the category still recognises the
  refusal a person now has enough detail to debug.
- Import a vendor profile whose timezone arrives nested under `id`. Everything
  else unreadable is still refused by name.
- Announce a field's label once. A labelled group around a labelled control
  gave assistive technology two names for one field; the group is named only
  when no control inside it took the name.
- Mark what the engine generates rather than presenting it as something the
  operator entered, and offer each country once.
- Refuse a fractional count in a flat profile edit - `cores`, `seed`, camera,
  microphone, speaker and touch counts, and memory outside 0.25/0.5 - instead of
  accepting it and storing a truncated number. The API, the batch and import
  routes and the dashboard store all read the rule from the model's field policy.
- Refuse an integer too long to print with the ordinary write refusal rather
  than a conversion error raised while formatting the message.
- Treat only "no such process" as proof that a process group is gone; any other
  probe error keeps the browser owned instead of reporting it stopped.
- Tie a data lock to the thread that holds it, so another thread can no longer
  pass as the owner of a transaction and replace the whole config underneath
  it; one `NUMERA_LOCK_TIMEOUT` deadline now covers waiting on another thread
  and another process, and an invalid value falls back to the default.
- Report what a health action actually did instead of only that it ran.
- Reach every section from the command palette, and drop the sidebar entry that
  had no screen behind it.
- Let a registered self-hosted Windows runner take the Windows jobs, so the
  platform gate is a deliberate run rather than a permanently red check.
- Split `proctree` into the tree itself, the Windows job-object layer
  (`winjob`) and the POSIX group and terminal layer (`posix_session`). Behaviour
  is unchanged and every name is still reached through `proctree`; the file was
  1016 lines, which is past the point where anything can be added to it safely.

## [0.36.0] - 2026-09-19

The bound a caller declares now holds when something goes wrong, which is the
only time a bound matters. Every fix here is the same shape: a rule that was
correct while nothing failed.

- End a helper's readers, reports and processes under **one** deadline. The
  serialization lock was acquired outside it, so a cleanup given fifty
  milliseconds could wait indefinitely for another one to finish; each nested
  termination took a fresh full budget rather than a share of the declared one;
  and when a unit could not be ended, closing its pipes waited for the writer
  that failure had just left running. What cannot be closed is now retained and
  named on the record instead of waited on.
- Own a spawned child from the instant it exists. An exception between the
  spawn and the attach left the process in local variables only, so cleanup
  found a record with nothing on it, released it, and then blocked closing the
  pipes of the child it had just written off.
- Read whether a reservation resolved, not the label on its record. A second
  shutdown asked whether a helper was still "starting" - which the first
  shutdown had already changed - and released a reservation that could still
  produce a browser.
- Validate an ownership report instead of trusting it. A bare number, a list, a
  non-numeric pid, an unsupported tree kind or invalid UTF-8 either raised out
  of cleanup entirely - leaving the reader threads alive outside it - or were
  accepted as a complete account of what a helper started.
- End the **tree** when its ownership cannot be recorded, not just its leader.
  The driver is in its own job or session by then, so killing the immediate
  process left its browser running under a message that said otherwise.
- Apply the vault completion rule to every path, including "already committed".
  A matching salt used to be enough to confirm a descriptor without the key,
  which accepted ciphertext written under a different key with the same salt -
  measured at **0 of 3** credentials openable afterwards - and left a config
  verifier that opens nothing in place while deleting the journal carrying the
  good one. Finishing a migration now needs the master password, on every path;
  a config and a journal that disagree are reported rather than guessed
  between.
- Look inside a secret-bearing container, not just at its shape. A proxy pool of
  `[1, 2, "x"]` is a list, so it passed - and the walker skips every element it
  cannot read, so the file reported no secrets at all and a migration completed
  over records nobody had examined.
- Choose the validation context from the caller, never from the field under
  test. A complete engine record whose fingerprint was null or missing was
  routed to the *partial* validator, which exists to permit exactly that
  omission.
- Store the device memory that was sent. `navigator.deviceMemory` is a double
  and real browsers report 0.25 and 0.5; the converter cast with `int`, so both
  became 0 - a legal amount, so nothing downstream noticed the value had been
  replaced. Those two are now preserved and anything else fractional is refused
  by name.
- Make the leak gate a gate. `error::ResourceWarning` had never failed a run:
  an unclosed file collected after a test, or a warning raised in a thread, is
  re-emitted by pytest under a different class that the filter did not name.
  Both wrappers are now errors, and a planted leak of each kind is a test.

## [0.35.0] - 2026-09-13

A refused vault migration now says *why* in a form something can act on, and
the refusals stopped disagreeing with each other.

- Give a refusal a cause, not only a sentence. `VaultIncomplete` carries the
  list of reasons the migration is not complete, each a stable code beside the
  text a person reads, so a route, the CLI or a test can ask which problem
  occurred without matching prose. A wrong password during a resumed rotation
  likewise reports which of the two was rejected.
- Stop answering "can this data root be read" in two places. An interrupted
  `vault enable` had its own check for unreadable secret files, written before
  the completion rule existed and still deciding first — so the one refusal a
  caller actually met was the one carrying nothing to match on. Both are now
  the same check, and it names its cause.
- Say something that can work. The refusal used to end by suggesting the master
  password whatever was wrong, including for a file that will not parse, where
  a password cannot help. The remedy now follows the cause.
- The exception types and cause codes moved to their own module, so matching on
  why a migration refused no longer means importing the protocol that produced
  the refusal.

Then CI ran for the first time in a long while — on a self-hosted runner, which
a billing block does not reach — and found three more, two of them the same
shape as the vault defect above.

- Never report a helper stranded because something else already cleaned it up.
  A server shutdown and the helper's own completion settle the same record by
  construction, and the guard against that only caught the second one arriving
  *after* the first had finished. One arriving *during* it ran the whole cleanup
  again, read an ownership report the winner had already consumed, called the
  absent file lost evidence — and reported a browser whose process had
  demonstrably exited as unaccounted for. Settling one helper is now one
  operation at a time, and a caller arriving after a proven one takes its
  answer instead of looking again. A helper that could *not* be proved clean is
  still retried by a later sweep, because the reason may since have gone.
- Stop asserting which of two correct refusals arrives. A launch whose browser
  cannot start is refused with the child's own reason, and that reason is "no
  executable" or "the package isn't installed" depending on what the machine
  has. The test pinned one, so it passed on two CI matrix legs and failed on the
  third; it now asserts what actually matters — that the answer is the child's
  diagnosis rather than the bare placeholder.
- Give the dashboard suite a timeout with real margin. Every test in it finishes
  well inside two seconds warm, but the first one to render the app in a cold
  worker pays the module graph inside its own measured window, and the 5-second
  default left nothing spare. Three tests were passing on how the run happened
  to be scheduled, and failed on a fresh install.

## [0.34.0] - 2026-09-13

A third recheck kept N14, N02 and N04 open. Each was reproduced at the real
project boundary with real processes and real cryptography before anything
changed, and two of the three reproductions were worse than the report.

- Stop truncating a helper's answer in silence. A probe whose browser driver
  inherited its pipe writers — the ordinary shape on Windows, where the driver
  is handed its own job handle and the standard handles come with it — wrote
  its last bytes without a newline, exited 0, and returned `stdout=''` to its
  caller alongside a successful exit code. Draining by line never yields a
  trailing write, the reader threads were daemons nobody joined, and the pipes
  were closed while a live process still held the other end so end-of-file was
  never reachable. Streams are owned now, every unit is ended before anything
  is closed, and output that could not be read to the end is an error rather
  than a shorter string. The same reordering took a measured 6.9s case to 3.8s.
- Never release a helper reservation that has not spawned yet. Shutdown settled
  a `starting` record, found no process on it and dropped it — after which the
  spawn went ahead and produced a child nothing tracked, behind a shutdown that
  had already reported success. Admission, spawn and attach are one state
  machine: a reservation is cancelled and waited for, and if it does not resolve
  it is reported stranded rather than forgotten.
- Tell lost ownership evidence from a helper that owned nothing. A missing
  report, an unreadable one and a truncated final record all read as "nothing to
  clean up", and a failure to *write* the report was swallowed in the child, so
  a browser whose ownership could never be communicated started anyway. A report
  that cannot be trusted now strands its helper, and a driver that cannot be
  recorded is stopped instead of started.
- Apply one completion rule to every vault path. Three separate shortcuts each
  decided "finished" their own cheap way: the enable path asked only whether any
  plaintext was left, so three credentials re-encrypted under a **foreign**
  descriptor answered no — it committed a descriptor that opened none of them
  and deleted the journal naming the right one, after which nothing on disk
  opened with any password. A nonempty but damaged verifier was committed
  without ever being used. A resumed rotation stepped over a plaintext pool
  password and reported success. Ordinary enable, ordinary rotation and both
  resumes now commit and then prove the root reached that generation, or roll
  back and keep the evidence.
- Answer, rather than crash, on a malformed descriptor. A salt that is not valid
  base64 reached the key derivation and raised `binascii.Error` straight through
  `check_password`, which every caller asks as a yes/no question.

## [0.33.0] - 2026-09-13

Two rechecks of 0.32.0. The first kept four findings open; the second kept three
of those still reachable at a boundary the repair had not covered. Every claim
was reproduced at the real project boundary before anything changed, and the
durability work turned up a defect neither recheck had found: on Windows the
protocol had never synced anything at all.

A recheck of 0.32.0 kept four findings open. Each was closed at the boundary the
previous fix did not reach, and each moved to a module of its own rather than
growing a file already past the size this project holds itself to.

- Tell a damaged recovery record from an absent one. A `vault-migration.json`
  holding invalid JSON, `[]`, `{}` or a salt that is not a string was read as
  "nothing to recover", so a data root with an unresolved migration opened as
  though nothing had happened — and calling recovery on one deleted it,
  destroying the only record of the salt its ciphertext is under. Four states
  now: absent, valid, unreadable and written-by-a-newer-build. The last two
  refuse, keep every byte, and say what to do.
- Stop inferring complete encryption from a scanner that skips what it cannot
  read. A corrupt `proxies.json` made an interrupted `vault enable` look
  finished, and the descriptor was committed over it. A secret-bearing file
  that will not parse now blocks completion by name.
- Propagate real disk errors while syncing, and sync in order. A failing volume
  was swallowed, so a migration could report success with nothing durably
  written; only the journal was ever synced, never the ciphertext it describes
  nor the descriptor before the journal was removed. Journal, then data, then
  descriptor, then cleanup — and only the documented "this platform cannot sync
  this object" errors are tolerated.
- Refuse an already-open store whose data root was left mid-migration by
  another process. The check at construction cannot see that; every secret read
  and write now does.
- Read the protection policy strictly. `null`, `0`, `""`, `[]` and `{}` are all
  falsey and every one of them used to open the API. Only an unambiguous JSON
  `false` does, and a credential file that is already damaged at startup is
  adopted into the policy like a healthy one — without that, deleting it
  afterwards turned an upgraded installation anonymous.
- Apply one write contract at every door. The API had a field schema and the
  core had a name check, so `tags: [{}]` was refused over HTTP and accepted by
  `numera import-from`. Text fields, list elements and numeric bounds are now
  the same question wherever it is asked, in a module with no optional
  dependency so the engine carries it without the server extra.
- Render a profile stored by an older build instead of blanking the page.
  Refusing new writes never repaired the installations that already held
  `name: 123` or `name: null`. The view renders what it finds; the stored bytes
  are untouched and no record disappears.
- Treat a helper as finished only when its process tree is provably empty. A
  helper that exited cleanly was accepted without asking whether the browser it
  started had exited too, a tree that refused to die was dropped by the only
  thing tracking it, and the shutdown hook discarded the list of what it could
  not stop. Stragglers are kept, named and reported, and no new helper may
  start once shutdown has begun.
- Compare API credentials as bytes. `compare_digest` on two `str` raises unless
  both are ASCII, so a hand-edited `tokens.json` could turn every authenticated
  request into an exception instead of a refusal.
- Pin Node explicitly in the Rust CI job, which was building the dashboard with
  whatever version the runner image happened to ship.

A further recheck kept three of those open. Each had been closed at one
boundary and left reachable at another.

- Commit a vault migration's data and its descriptor as one transaction. The
  descriptor was written through a callback that sat outside the migration's
  undo batch, so a failure after it landed rolled the secret *files* back to
  the old generation and left the config naming the new one. The retry then
  read the matching salt as a finished job, deleted the journal and reported
  success — over three credentials that no password in existence could open.
  There is now one commit point, used by `enable`, by rotation and by the
  resume of either, and a salt that matches is no longer accepted as evidence:
  the records are opened before the journal is allowed to go.
- Actually sync on Windows. `os.fsync` there needs a handle opened for writing
  and answers `EBADF` on a read-only one, and `EBADF` was on the tolerated
  list — so every sync the recovery protocol performed on Windows opened the
  file, was refused, and returned as though the data were on the platter. The
  durability the whole protocol rests on was not being delivered at all on the
  platform the product ships for. A directory handle keeps the wider tolerance
  it genuinely needs; an ordinary file no longer does.
- Refuse a journal that carries a salt and nothing else, and a secret file
  whose container is the wrong shape. A descriptor with no verifier produced a
  vault no password could be checked against; `proxies.json = null` parses,
  holds no secrets to find, and read as a finished migration.
- Validate the shape a profile is actually written in. The shared contract
  checked `cores`, `memory` and `seed` at the top level, where an engine
  profile keeps them one level down under the browser's own names — so
  `fingerprint.seed = inf` was written by the core path while the flat
  equivalent was refused over HTTP. The proxy block had no core check at all.
  One set of bounds, three shapes, and the write intent a proxy carries is read
  and never reassigned.
- Give a record with no `name` at all something to render. The repair helped a
  name that was present and wrong; a missing one still reached
  `p.name.toLowerCase()`, which blanks the page for every profile rather than
  the one.
- Give each server its own helper registry. It was a module global, so one
  app's shutdown sweep reached another app's helpers and the first sweep
  anywhere closed the door on every later one. Admission is now reserved before
  a child is spawned rather than checked after, a clean exit with an unproved
  cleanup is reported as a failure instead of a result, and a browser driver in
  its own session — which on POSIX is not in the helper's process group at all
  — is ended along with the tree it left.
- Bound the read as well as the wait. A driver that inherits the helper's pipe
  writers keeps them open after the helper exits, so waiting for end-of-file
  made a finite probe's deadline the browser's lifetime.

## [0.32.0] - 2026-09-12

Seven of the seventeen findings `0.31.0` reported closed were only partly
closed. A recheck found each of them still reachable at a boundary the first
fix did not cover; every one was reproduced here before anything changed.

- Finish an interrupted vault migration, or refuse to serve the store — never
  commit the descriptor over whatever happens to be on disk. The journal
  covered one crash point: a kill after the *last* secret file. A kill anywhere
  earlier left credentials in plaintext and the next open marked the vault
  enabled over them, which claims a protection that is not there. Recovery now
  reads the disk: an enable whose files all landed still completes unattended,
  one with plaintext left is finished with the master password or refused.
- Journal the master-password rotation too, with both descriptors. It recorded
  nothing at all, and the old `recover` deleted any journal whenever the config
  named a vault — which during a rotation it always does — so a kill destroyed
  the only copy of the new salt. `numera vault resume` finishes one; it needs
  both passwords, because both states are ciphertext and nothing short of a
  trial decryption tells them apart.
- Keep the journal when a *resume* fails. Rolling back a resume restores the
  half-migrated state a crash left, not a clean one, and that state still needs
  the salt.
- Record the authentication policy for data roots created before it existed.
  Only minting a token wrote the marker, so an installation upgraded from
  0.30.1 was protected only while its `tokens.json` stayed readable — deleting,
  emptying or corrupting it turned the API anonymous. Existing credentials are
  adopted into durable policy before the first request is served, and an
  unreadable credential list or marker now means protected, not open.
- Apply one write contract at every entry point. Three still bypassed it: an
  integer too large for a float crashed the validator instead of being refused
  by field, bulk tag operations raised on a non-string element before anything
  checked it, and the vendor import wrote converted records with no schema
  check at all — so `{"name": 123}` reached the dashboard, which calls
  `.toLowerCase()` on it. Imports are refused whole, before either store is
  written, naming the profile and the field.
- Close the CLI import door as well. `numera import-from` is a fourth way in
  and had no guard; the check now sits at the single primitive every engine
  profile write reaches, so all four doors close at once.
- Stop splitting a valid URL at an embedded comma. A scheme appearing after a
  comma was read as a separator wherever the comma sat, so
  `?targets=https://a,https://b` became two startup tabs and the path segment
  `red,blue:green` became a second host. A comma now separates entries only
  while the text before it is still a scheme and a host. An explicit list is
  taken whole: a caller that sent a list already said where each entry ends.
- Finish cleaning up after a browser helper, not just bound its wait. A timed
  out helper left both pipes open, every other failure path ran no cleanup at
  all, and nothing tracked a running helper — so a server that stopped
  mid-probe left its browser behind. Helpers have their own registry and their
  own shutdown hook now, beside the one that stops sessions.
- Probe the portable data directory by writing to it. `create_dir_all` returns
  success for a directory that already exists and cannot take a new file, so a
  read-only or ACL-denied location was passed to the engine as an explicit
  `--data-dir` it accepts verbatim. An unavailable portable location now falls
  back to the per-user one by name, never into the engine directory an update
  replaces.
- Resolve the selection once for every bulk action. Edit all used the raw
  selection while the toolbar used the resolved one, so a profile deleted in
  another client left the toolbar reading "1 selected" above a modal offering
  "Apply to 2". A partial edit is now reported as one.

## [0.31.0] - 2026-09-12

Seventeen findings from one audit. Four of them could lose data or open a door
on their own: a portable build that could not launch anything, a vault
migration whose salt died with the process, an API that went anonymous when it
lost its tokens, and a profile listing one bad record could take down. Each fix
landed with the test that failed before it.

> **Correction.** This entry originally said the seventeen were "closed
> together". A recheck of this release found seven of them only partly closed:
> the vault journal covered one crash boundary of several and no rotation at
> all, the authentication policy was never recorded for data roots created
> before it existed, three write paths still bypassed the field schema, valid
> URLs were still split at embedded commas, the helper deadline ended the wait
> without ending the helper's pipes or ownership, the portable data directory
> was probed with a call that cannot detect an unwritable one, and one bulk
> edit still used an unresolved selection. See the next release for what
> closing them actually took.

- Build every self-spawn through one frozen-aware command builder. A frozen
  build's `sys.executable` is the application, not a Python interpreter, so
  `-m numera` reached Numera's own CLI and every launch, stop, attach, warm-up,
  probe, schedule and detached job failed in a portable build.
- Give the finite browser-backed helpers an operation deadline
  (`NUMERA_HELPER_TIMEOUT`, default 300s). A stalled helper is ended together
  with the browser tree it started and the route answers 504, instead of
  holding the request open indefinitely.
- Bulk Launch no longer stops already-running profiles: it is launch-only and
  treats a running session as skipped.
- Fetch extension icons through the authenticated client and render them from a
  managed blob URL, so they appear when the API requires a bearer token.
- Report per-profile outcomes from bulk warm-up instead of swallowing every
  failure and reporting success.
- Resolve every bulk action, the selected counter and the delete confirmation
  against the same global selection, so a filtered view can no longer delete
  fewer profiles than the confirmation named.
- Match tag search case-insensitively on both sides, and survive a non-string
  legacy tag.
- Include all four launch backends in desktop builds, with separate Playwright
  and Patchright binaries, Camoufox resources and GeoIP data. Local/internal
  builds can additionally include the Cloak binary explicitly.
- Preserve an explicitly cleared password in the pooled-proxy editor.
- Restore known proxy credentials when explicitly keeping a whole-proxy conflict
  after remote removal; refuse restoration when the credential is unavailable.
- Limit release downloads and publication to the four required product artifacts,
  failing when one is missing and excluding diagnostic artifacts.
- Close entered Camoufox backends when driver ownership is refused before the
  browser is yielded, retaining both failures and the claim if cleanup fails.
- Recover stale Dashboard revisions by rebasing only local edits onto the latest
  profile, with explicit choices for overlapping changes and preserved proxy
  password/removal intent. API conflicts now carry a domain `kind`.
- Require all four launch engines on Windows and Linux in CI, reject missing or
  skipped live coverage, and gate tagged releases on successful verification.
- Preserve engine-only proxies, including passwordless endpoints, when a
  Dashboard listing or unrelated edit carries no visible proxy.
- Allow independent profile preparation while serializing actual driver
  creation and durable ownership recording.
- Keep rejected profile edits open with their draft and password intent;
  prevent duplicate pending saves and distinguish committed writes from failed
  post-save refreshes.
- Keep denied test connections within the socket error contract so client
  libraries close refused sockets, with a regression that verifies cleanup.
- Include shared test helpers and the network guard in the source distribution.
- Isolate portable smoke data from automatic imports of existing profiles.
- Journal a vault migration before it writes, and roll it forward on the next
  open. A migration killed between encrypting the secrets and committing the
  descriptor left ciphertext whose salt existed only in the dead process's
  memory. The journal holds the salt and verifier — never the password and
  never a plaintext secret — and is cleared once recovery commits.
- Keep a protected API protected when its credentials are lost. Revoking the
  last token, deleting `tokens.json` or corrupting it turned a locked API
  anonymous with no restart and no warning, because the policy question was
  being answered with a credential count. Protection is now its own marker,
  turned off only deliberately, and revoking a lone token is refused with a
  409 rather than silently opening the door.
- Keep a portable build's data beside the application instead of overriding an
  explicit `NUMERA_DATA_DIR`.
- Type every field a client may write, not half of them. Wrong-shaped list
  entries and non-finite numbers reached disk, where `NaN` broke serialisation
  for the whole profile listing and a non-string tag blanked the page. Writes
  are refused by field name; the two read-only overviews survive a record
  written before the schema existed.
- Refuse a profile the engine cannot model instead of storing the dashboard
  copy alone. Such a write answered 200, showed the profile in the GUI and
  left `numera list` with nothing; it now answers 422 naming the reason.
- Answer a bulk proxy check whose targets are all skipped, instead of 500.
- Read Numera's own profile export as Numera's own format. A native file fed
  through the vendor importers lost its fingerprint seed, signal modes,
  startup URLs, extensions and folder, and reported the result as a faithful
  import. Exports now carry a format version, the import route reads it field
  for field, and an unversioned native file is refused by name on both the API
  and the CLI door rather than quietly reinterpreted.

## [0.30.1] - 2026-09-09

Closing what 0.30.0 left open. No engine behaviour changed: this is the
verification catching up with the code, plus one dependency.

### Changed

- **Every launch backend is now exercised for real.** `patchright`, `camoufox`
  and `cloakbrowser` were absent from this machine and from CI, so those three
  engines were covered by a wiring assertion and nothing else. They are
  installed, and `test_session_tree_live.py` opens a real browser on each
  installed engine and proves it lands in a tree of its own - parametrised over
  what is present, so an engine that is missing is visibly absent from the
  report rather than silently unclaimed. All four pass on Windows.

- **A leaked handle fails the suite.** `error::ResourceWarning` is on. It was
  measured before it was turned on - the suite already passed with it - so
  nothing is being suppressed to keep it green. It found two real leaks
  immediately: an unreaped child in an ownership test, and the SOCKS4 fake
  leaving both ends of a completed tunnel to the garbage collector. Warnings
  raised from `__del__` on a daemon thread can still only warn, which is
  Python's limit, not a policy choice.

- **The test process is confined to this machine.** A new `tests/conftest.py`
  scrubs `NUMERA_DATA_DIR`, `NUMERA_API_TOKEN` and the proxy environment
  variables before any test module loads - a data dir pointing at real
  profiles, or an inherited `HTTP_PROXY`, changes what a test operates on and
  reaches - and refuses outbound sockets to anything that is not this machine.
  It found a live one: the SOCKS check test counted its tunnel only after the
  upstream dial succeeded, so an assertion its own docstring called independent
  of the internet in fact required it.

- `lucide-react` 1.41.0 -> 1.43.0.

## [0.30.0] - 2026-09-09

The twenty-first audit brief. The session tree stops being best-effort: every
browser this project opens either owns one, or is refused. And taking a proxy
away becomes an act with a name, instead of a statement about a password that
was no longer there.

### Fixed

- **A session's own tree was optional in practice.** 0.29 captured one when it
  could and fell back to the whole host process when it could not - a package
  that moved, a job object that would not be created or assigned, a backend
  with a different launcher. A correctness primitive that degrades silently is
  not one: the fallback is exactly the state it was built to end. Each of those
  is now a refusal, and any driver already started is stopped. The `cloak`
  engine joins the same seam rather than sitting outside it.

- **The tree was recorded only after the launch call returned.** Playwright
  starts its browser well after `start()`, so the window was invisible there -
  but Camoufox opens one inside `__enter__`. A launcher hard-killed in that
  window left a running browser whose only durable record still named the host
  tree, with nothing pointing at the group it was actually in. The claim is
  narrowed from inside the spawn now, while the driver is still only a driver;
  a claim that cannot be narrowed stops the driver instead.

- **Removing a proxy was read as preserving its password.** `proxy: null` is
  neither preserve, clear nor replace - it takes the proxy and the credential
  together - and reading it as "said nothing" was wrong twice over. It refused
  what the user had just decided: with two readable passwords the copies
  disagree, and deleting the proxy settles that, yet the write came back 409.
  And it let through what it should have refused: under a locked vault an
  explicit clear raises `VaultLocked`, but a removal carries no password field
  for that rule to look at, so the same destruction went through unread.
  `REMOVE` is that act, named, and the HTTP and Python surfaces answer it
  identically. A proxy only the engine copy holds is still refused: the listing
  never showed it, so `null` was not a decision about it.

- **The Dashboard's password marker gained lifecycle coverage** across the
  transitions it has to survive - toggling the proxy off, off and on again,
  picking a pooled proxy, and the edit after a save - so the omission rule
  cannot regress to inferring intent from value equality.

## [0.29.0] - 2026-09-09

The twentieth audit brief. A browser session now owns an OS tree that holds it
and nothing else, so a profile is released from proof rather than from a report
— and the claim, the Dashboard and the proxy pool each stop guessing at
something they can be told.

### Fixed

- **A session's ownership unit was the whole host process.** `own_tree()`
  caches one tree per process, so every in-process session named the same one
  — sibling profiles and every unrelated child of an embedding application
  included — and it could not answer the only question that makes a profile
  safe to reuse. A browser is started by a driver process and everything that
  driver starts inherits its group or job, so the tree is decided at that
  spawn: each session gets its own (a POSIX session, a Windows job object), the
  claim is narrowed to it as soon as it exists, and the claim is given up only
  once the OS says that tree is empty. A browser left running past a clean exit
  now keeps its profile; a body that raised over an already-closed one does
  not. The `cloak` engine still uses the host tree — its driver is not
  Playwright's — and says so.

- **Two threads could adopt a tree at once.** `own_tree()` cached, but the check
  and the adoption were separate steps: two first-opens landing together made
  two job objects or two `setpgrp` calls, and the callers then held two answers
  to a question with one.

- **The Dashboard turned every edit into a password replacement.** It loads
  profiles with the proxy password decrypted and PUT the whole object back, and
  a password in a write means REPLACE — so editing only the notes of a
  profile whose two stored copies disagree silently chose the Dashboard's
  credential instead of the 409 the server owes. The field is now sent only
  when the password control was actually used; never inferred from the value,
  because retyping the same password and never touching the field look
  identical and mean opposite things.

- **A claim naming no checkable tree read as free.** A 0.26-era leader-only
  record (`kind: ""`), a kind this build does not know, or the other platform's
  kind all fell through to "is the leader alive" — so an older launcher
  killed while its Chromium survived handed a live profile directory to the
  next opener. A browser-session claim is now evaluated only when it names a
  durable tree *this* platform can ask about; anything else is unknown, which
  every caller treats as owned, and the message says the record is the thing to
  repair. Empty session ids and malformed tokens and groups are unknown too.

- **Camoufox left a claim standing after a clean close.** It closes from
  `__exit__`, and an exception in the body carried past the line that recorded
  the close — so a browser that had shut down cleanly kept its profile
  busy. The exit protocol is driven explicitly now: `__exit__` still decides
  whether to suppress, and an `__exit__` that raises still keeps the claim.

- **A pooled proxy accepted a password that was not text.** `{"pass": 123}`
  was stringified on create and passed through raw on edit, reaching the vault
  writer as an int. Type and intent are separate questions: omitted preserves,
  `""` clears, a string replaces, anything else is a 400 — on both routes.

- **Windows handle calls went through unbound ctypes signatures.** `OpenProcess`
  returns a pointer-sized handle and ctypes defaults an unbound return to 32
  bits, so `alive()` and `started_at()` — the liveness checks the durable
  claim rests on — truncated it. Every `kernel32` call this project makes
  now declares its own types, from one cached library rather than a fresh one
  per call that binds nothing anyone else can see.

## [0.28.0] - 2026-09-09

The nineteenth audit brief. What a session leaves behind is now answered from
what actually happened to its browser, one secret-write contract covers the
bulk path as well as the single one, and a record nothing can read is owned.

### Fixed

- **A failed session left the profile owned by nothing.** "The yield happened"
  was read as "a browser is running", which is true of a caller that raises
  before it opens anything and of one that raises after its own `finally` has
  closed the browser again — both left a claim standing until a person
  deleted the file by hand. The backends now say what happened: a claim is kept
  only for a browser that was opened and not closed, and a close that raised
  never reports one.

- **Two sessions on one thread answered for each other.** The record of "did a
  browser open" was per thread, so a second profile opened inside the first
  session's body reset it — and the outer session, whose browser was still
  up, gave up its claim on the way out. Each session keeps its own.

- **Adopting a process group could move the host and then fail.**
  `os.setpgrp()` succeeds, and the `signal.signal` that follows raises
  `ValueError` off the main thread: an application whose first browser open
  happened on a worker thread was left in a group it never asked for, outside
  the terminal's foreground group and therefore deaf to Ctrl+C, over an
  exception the caller did not recognise as a refusal. Adoption is now all or
  nothing, and serialised, so two threads cannot both take one.

- **`POST /api/profiles/bulk` bypassed the secret contract the single write
  had.** It went through a second write path with no conflict refusal and no
  per-copy ciphertext preservation, so an unrelated notes or tags edit across a
  selection could do what one profile at a time could not. Both paths now share
  one secret-aware prepare step.

- **An explicit clear was lost before it reached the vault.**
  `proxypool.normalize()` maps both `None` and `""` to `None`, which is the
  right canonical *data* and the wrong *intent* for a secret, and
  `PUT /api/proxies/{pid}` copied the normalised value back over the caller's
  — so `{"pass": ""}` answered 200 and left the old password in place. The
  address fields take the canonical values; the password keeps the caller's.

- **`ProfileDiverged` and `VaultLocked` reached HTTP as 500s.** A refusal the
  store makes on purpose told a client that Numera had broken. They are
  answered as **409 Conflict** and **423 Locked**, from one app-level handler
  rather than per route.

- **A claim file that could not be read was treated as absent.** "No claim" and
  "a claim I cannot parse" were one answer, and the next opener overwrote what
  may have been the only surviving record of a browser whose launcher died.
  Unreadable is now its own state and counts as owned — for opening,
  deleting, backing up and restoring — and every field of a claim is
  validated, so a malformed pid, group, kind or session id is a controlled
  refusal rather than a traceback.

### Known limitation

- A session's ownership unit is still the process tree, not a tree of its own.
  For direct/foreground/Python opens that tree also contains sibling sessions
  and unrelated children of an embedding application, so "this session's
  browser is gone" is answered from what the backends report rather than from
  proving that exact tree empty. Server and detached launches each get their
  own tree already.

## [0.27.0] - 2026-09-09

The eighteenth audit brief. Two identity boundaries: **one profile has one
session-specific owner**, and **one secret write has one explicit intent**.

### Fixed

- **A launcher that never led a process group owned nothing.** POSIX used the
  process group only when the process already led one — which an interactive
  shell provides and a script with job control off, cron, systemd or an
  application importing the Python API does not. Those got a claim naming the
  dead leader, so killing the launcher left its Chromium running while the
  claim read free. The process now makes a group of its own and hands the
  terminal to it, so Ctrl+C still arrives, and a session that cannot be given a
  real tree is refused rather than started.

- **Two profiles open in one process held each other.** Both claims named the
  same process tree, so closing the first found the second's browser in it and
  kept the first profile owned — sometimes until the host exited, and an
  unrelated child of an embedding application did the same. A claim belongs to
  the session that took it, and is given up on a clean end.

- **A destructive operation trusted its own claim.** `busy()` answers "is
  somebody *else* using it", which is right for a listing and wrong for a
  delete: a session whose browser would not close leaves its claim standing on
  purpose, and that is exactly when the directory must not be renamed away.
  Delete, backup, restore and session delete now ask whether anything holds it.

- **Public `delete_session()` still only took the OS lease**, which is the one
  thing a killed launcher does not leave behind. It takes the durable check
  too, and the unguarded `detach_session`/`restore_session` primitives are
  internal.

- **An unrelated edit could destroy a credential the other copy lacked.** With
  a password on one side and nothing on the other, the empty side was mirrored
  over the working one. Present-on-one-side is now an unresolved conflict for
  any write that says nothing about the secret — except in the direction where
  nothing is destroyed: an engine copy that lost its password gets it back from
  the authoritative one.

- **A locked edit normalised two ciphertexts to one.** Under a locked vault two
  blobs may be one password encrypted twice or two different passwords, and
  this process cannot tell — so it now decides nothing: non-secret edits go
  through and each copy keeps its own bytes.

- **A proxy password could not be cleared while keeping the proxy.** Empty meant
  "keep". A write now carries intent: withheld (`None`) preserves, empty (`""`)
  clears, anything else replaces. Clearing a stored secret requires an unlocked
  vault, because destroying a credential this process cannot read is the one
  irreversible thing to get wrong; clearing nothing is always allowed.

- **Claim cleanup is one transition** — `sessions.settle()` — used by the
  server's stop and shutdown, detached `jobs stop`, `numera stop`, the
  scheduler's reaper and a successful delete.

## [0.26.0] - 2026-09-08

The seventeenth audit brief. 0.25.0 made ownership mean the whole browser tree
*inside the process that started it*. This pass makes it survive that process,
and makes the two checks that only some callers ran into gates all of them
share.

### Fixed

- **A killed launcher released the profile while its browser kept running.**
  The session lease is an OS file lock, and the kernel drops it when the holder
  dies — so a crashed or `kill -9`'d launcher left a Chromium holding
  `user-data/<id>` with the lease free, and the next opener took it and started
  a second browser on the same directory. Ownership is now also written down
  (`numera/sessions.py`): a claim naming the *tree*, taken before a browser can
  open and removed only once nothing that session started is left. A later
  opener asks the OS whether that tree is still occupied — occupied refuses,
  provably empty recovers, and anything it cannot check refuses. `numera stop`,
  `/stop`, the API's status, delete, backup and restore all consult it, so the
  state that used to report "nothing running" now reports what is running.

- **A foreground `numera launch` owned nothing at all.** It is not spawned
  through the tree abstraction — it is the user's own CLI process — so a hard
  kill left a browser with no owner, no record and no lease. It now adopts a
  tree for itself: a job object on Windows (which takes the browser with it,
  because the OS does), or its own process group where the shell already gave
  it one.

- **Tree ownership degraded to leader-only instead of refusing.** If a job
  object could not be created or assigned, the session was started anyway with
  no durable handle on its descendants — the original defect, reached by
  another route. A session that cannot be owned is now stopped before it opens
  anything.

- **A job object that could not be queried read as empty.** "No such job" and
  "the query failed" arrived as the same answer, so one failed query declared a
  live browser tree gone. Unknown is now owned.

- **A direct launch bypassed the divergence refusal.** Two copies of a profile
  holding different proxy passwords made the API refuse and let
  `numera launch` proceed with the engine credential, because only the API
  reconciled first. The check moved to `drivers.open_session`, the gate every
  browser open already goes through.

- **An ordinary edit settled that conflict by destroying one of the
  passwords.** `resolve()` returned the engine copy with a warning, and the
  next unrelated change — a note, a rename, from the CLI or the dashboard —
  wrote that whole object through both files. A write that does not explicitly
  replace or clear the credential is now refused; one that does resolves it and
  converges both copies.

- **A name lookup could answer with a different name.** `resolve("beta")`
  reached the id through the dashboard's name and, with the repair refused,
  returned a profile called `alpha`.

- **A full config replacement deleted concurrent changes.** Taking the root
  transaction inside `save_config()` closed the lock-domain hole but not the
  stale snapshot: the caller's whole-document read happened before any lock, so
  anything committed in between was erased by the write, and both callers were
  told they had succeeded. Full replacement is now private and requires the
  transaction that produced the snapshot; `set_config_many()` is the public
  mutation API and reads under the lock it writes under.

- **Engine-only legacy records were listed but never repaired.** The missing
  dashboard twin was created only by an HTTP listing, so a CLI-only machine
  could list and resolve a one-sided profile forever. Adoption preserves the
  engine record byte for byte and carries the stored secret across as stored,
  so a locked vault is no obstacle.

### Changed

- Live browser suites skip with a reason when no browser can start, instead of
  failing. A machine with the package and without the browser's system
  libraries produced a page of red that said nothing about the code.

## [0.25.0] - 2026-09-07

The sixteenth audit brief. 0.24.0 made every stop prove the process it could
see had died. This pass is about what that process *starts*: a `numera launch`
is a Python interpreter, and the browser holding the profile directory is three
processes further down.

### Fixed

- **"Stopped" proved the launcher, not the browser.** The API, the scheduler
  and every `--detach` job spawned a plain subprocess and asked only whether
  that one process had exited. If Python died while a Chromium it started did
  not, the registry entry was dropped, the session lease went with the dead
  process, `/stop` and `jobs stop` recorded "stopped" — and the next launch took
  the free lease and opened a second browser on the same `user-data/<id>`,
  which is the corruption the lease exists to prevent. Every session is now
  spawned as an owned process tree: a job object on Windows, its own session
  and process group on POSIX. Nothing releases ownership, writes `stopped` or
  treats a profile as free until the whole tree is proved gone, and the group
  handle outlives the leader so a survivor can still be found and killed. New
  module `numera/proctree.py`; the server, the scheduler, the CLI, the MCP job
  tools and the desktop shell's death cleanup all route through it.

- **A missing process identity authorised a signal.** A job recorded
  `pidToken: None` whenever the start time could not be read, and `jobs stop`
  then fell back to matching on the pid alone — so a recycled pid could send
  `taskkill /T /F` to an unrelated process tree. Identity is captured at spawn,
  retried only while the child is known to still be the one just created, and
  recorded with the group handle. Without it the job is marked unverified and
  `jobs stop` refuses rather than guessing; a child that exits before it can be
  named has its outcome recorded there and then, so nothing later reaches for
  its number.

- **A zombie read as a running browser.** On POSIX a process that has exited
  but not been reaped stays in its process group and goes on answering
  `killpg(pgid, 0)`, so a group holding nothing but corpses looked like a live
  session and no stop could ever prove otherwise.

- **`ProfileStore.list()` was engine-only.** A profile an older version wrote
  to the dashboard and not to the engine was visible in the GUI, launchable
  through the API and resolvable by id or by name — and missing from
  `numera list`. Listing is now the union of both stores under the root
  transaction, and a one-sided record is repaired rather than skipped.

- **Two copies of one profile that disagreed were never reconciled.**
  `find_by_name("beta")` found the id through the dashboard's name and returned
  the engine record, still called `alpha`. There is one policy now: for every
  field both shapes carry the dashboard record is the authority, and `list`,
  `resolve(id)`, `resolve(name)`, the API listing and launch all converge on
  it. A field only the engine models — the viewport, which the dashboard shape
  has no room for — is not a disagreement and is no longer reset by the
  derivation, which also stops a launch quietly changing the fingerprint it
  launches. Two different proxy passwords are the one case a derivation may not
  settle: the stored credential stands and `ProfileDiverged` says so.

- **Public `delete_session()` bypassed the session lease.** `detach_session` is
  a rename, and on POSIX renaming a directory a process has open succeeds, so a
  Python caller could move `user-data/<id>` out from under a browser another
  Numera process was running in it. It takes the lease, and reclaims the
  detached tree after releasing it.

- **Public `save_config()` bypassed the root transaction.** It took the config
  file's lock alone, making it a lock domain of its own: a vault migration
  holds the transaction, reads the config, does minutes of work and saves it
  back, and an outside settings save could write into the middle of that, be
  told it had succeeded, and then be overwritten by the read-modify-write it
  interrupted.

### Changed

- `numera jobs list` shows `orphaned` for a job whose process exited while
  something it started is still running, and `jobs show` says when a job has no
  verified identity and therefore cannot be stopped by Numera.

## [0.24.0] - 2026-09-06

The fifteenth audit brief. 0.23.0 fixed the coordinator and session paths; this
pass is the callers and the recovery branches that still stepped around them —
and one promise the code was making without being able to keep.

### Fixed

- **"Stopped" now means proved stopped.** `end_child` answered `None` when a
  signal was refused or the process was still unreaped after the kill, and
  every caller read that as an ordinary exit: `/stop` returned 200 for a
  browser that was still running, a schedule recorded "warm-up exited None",
  and the registry entry was dropped while the child still held the profile's
  session lease. It returns a real exit code or raises
  `ChildTerminationFailed`; a child that cannot be killed stays owned and
  visibly running, and a startup child that cannot be reaped is reported as an
  orphan instead of the profile being called free.

- **Detached jobs had the same defect in a worse place.** `numera jobs stop`,
  `numera stop` and the MCP job-stop sent one `SIGTERM`, ignored the result and
  wrote `state=stopped` — over a detached process and browser that were still
  alive and still holding the profile. Termination now re-checks the pid token
  before every signal, waits, escalates to a forced tree kill, waits again, and
  records the outcome only once death is proved. If it cannot be proved the job
  stays visibly running and the caller is told.

- **`numera stop` no longer says "nothing running" about someone else's
  session.** The job registry only knows what this machine's CLI detached; a
  profile held by a server, a desktop or another CLI now reports that this
  command does not own it.

- **A browser startup releases everything it acquired.** The guard began after
  the Playwright driver had started, so an authenticated proxy's relay — opened
  before it — was leaked whenever the driver itself failed. And `_abandon`
  swallowed every cleanup error, so "no browser survives a failed startup" was
  a promise made while discarding the evidence that `close()` had failed. The
  guarded region now runs from the first acquired resource through READY, and a
  cleanup that fails raises `StartupRecoveryFailed` carrying both the original
  failure and the steps that could not be undone.

- **Legacy name ambiguity is a two-directory question.** `AmbiguousProfile` was
  raised from a scan of `profiles/` alone, so an engine profile called `same`
  beside a dashboard-only one called `same` looked unambiguous and a name-based
  delete went ahead. Both directories are searched now, an id present in both
  counts once, and a dashboard-only profile is repaired into a complete pair
  rather than denied — answering "no such profile" for an id the GUI is showing
  was the engine disagreeing with the interface in front of the user.

- **The CLI deletes what you named.** `delete alpha` resolved a profile and
  then deleted it *by id*, which discarded the coordinator's own protection: by
  the time the lease was taken, `alpha` could name a different profile. The
  original reference is carried through to the delete, so a target that moved
  is refused. Repeated references to one profile delete it once, and a delete
  that removed nothing is no longer reported as a deletion.

- **`--detach` errors like every other command.** Its target was resolved
  before the CLI's error handler, so an ambiguous legacy name printed a
  traceback and created no job record, while the same command in the foreground
  printed a clean JSON error.

- **Reclaiming a deleted or replaced session no longer freezes the data root.**
  Both ran `rmtree` over a whole Chromium profile while holding the root
  transaction, blocking every unrelated profile, config and vault write long
  enough to reach the lock timeout. The commit and the garbage collection are
  separate now: locks are released once the metadata is final, and the detached
  tree is reclaimed after — with the same warning naming the exact path if it
  cannot be.

### Changed

- `ProfileStore.save_engine`/`delete_engine` are now `_save_engine`/
  `_delete_engine`. They were documented as internal but looked public, and
  they are the one way to create the one-sided records this release teaches the
  store to detect.
- `POST /api/profiles/{id}/stop` can return `500` when a child will not die,
  and `DELETE` refuses with `409` rather than deleting a profile whose browser
  is still running.
- The settings half of `ProfileStore` moved to `numera/config_store.py`.
  Nothing changed for callers; `store.get_default_engine()` and the rest are
  where they were.

## [0.23.0] - 2026-09-06

The fourteenth audit brief. 0.22.0 made the coordinator and the session
protocol strong; most of what is left is the edge of them — the public API that
never went through the coordinator, the data an older version already wrote,
and the windows the 0.22.0 changes opened behind themselves.

### Fixed

- **The documented Python API bypassed the coordinator.** `numera.__doc__`
  tells a script to call `store.save(Profile(...))`, and that wrote the engine
  record alone. So a profile created by a script was engine-only until a
  dashboard listing happened to adopt it; an *update* to an id the dashboard
  already knew never reached the GUI at all, because the listing only adopts
  ids it has not seen; and a delete left the twin behind, so the GUI went on
  showing a profile the script had been told was gone. `save` and `delete` are
  now the coordinator, with `save_engine`/`delete_engine` kept as the internal
  primitives the dashboard mirror needs.

- **Upgrading did not make an old data root safe.** 0.22.0 stopped duplicate
  names from being *created*; `find_by_name` still returned the first match in
  directory order, so on a store that already had two — an older Numera, a hand
  edit, a restored backup — `numera delete <name>` destroyed an arbitrary one
  of them and reported success. Name lookup now fails closed with
  `AmbiguousProfile`, naming both ids. Lookup by id still works, which is how
  the ambiguity gets fixed, and listing still works, which is how it is seen.

- **A browser that failed to start left itself running.** Moving readiness past
  real context creation was right, and it opened a window behind it: the init
  script, the client-hint overrides and the first page all run after the
  browser exists and before the parent is told anything. A failure in any of
  them left a live Chromium, a running Playwright driver and a relay port
  behind for a session correctly reported as failed — and even a failed launch
  never stopped the driver it had just started. Startup is now transactional in
  its resources: everything acquired before READY is released on every
  exception, so the profile's lease is free for the next attempt.

- **A killed child was not waited for.** `terminate()` escalated to `kill()`
  and returned without waiting again, so the exit code read straight afterwards
  was `None` for a child that had not been reaped. A schedule recorded
  "warm-up exited None", the registry entry was dropped while the child still
  held the session lease, and `/stop` reported success on a profile that was
  still busy. One helper now terminates, escalates and waits, and returns a
  real code only once the process is gone.

- **A scheduled warm-up could outlive its own occurrence.** The scheduler waits
  for a real browser and then writes `lastRun`. If that write failed, the claim
  was released and the warm-up left running — so the row was immediately due
  again and the next tick started a second browser beside the first. The child
  is now stopped before the claim is dropped, and if it cannot be stopped the
  claim is kept rather than the run repeated.

- **Restore discarded a session it could not reclaim.** The replaced session is
  moved aside as `.replaced-*` and removed with `ignore_errors=True`, so a
  cleanup failure left a whole browser identity on disk with nothing naming it.
  It is reported now, with the exact path, exactly like a delete tombstone.

- **`get()` raised where it should have said "no such profile".** The existence
  check ran outside the transaction the read then took, so a delete committing
  in the gap produced a `FileNotFoundError` instead of a clean miss. Both are
  inside one transaction now.

- **The engine store's withheld check straddled the vault boundary.** The
  dashboard store and the proxy pool joined the snapshot in 0.22.0;
  `ProfileStore.secret_withheld` still read the record's bytes unlocked and
  judged them against `config.json` afterwards, comparing a stored value to a
  key state that never coexisted with it. It is the check several operations
  use to refuse rather than produce a confidently wrong backup or export.

- **A delete could act on a target that had moved.** `delete("alpha")` resolved
  a profile, took its lease, and deleted it by id — so a rename committing in
  between let both operations report success with no ordering that explains
  them. The reference is re-resolved under the lease now, and a name that has
  moved to another profile is refused with `TargetMoved` rather than deleted.

### Changed

- `ProfileStore.save()` enforces the same name-uniqueness contract as every
  other surface; a duplicate raises `NameTaken`.
- Name lookup can raise `AmbiguousProfile`, and `ProfileRepo.delete` can raise
  `TargetMoved`. Both are `RuntimeError`s, so the CLI renders them as a message
  rather than a traceback.
- Enabling or rotating the vault reports a higher count for a store built by a
  script. It already sealed both copies of every profile; a script-created
  profile simply has two of them now instead of one.

## [0.22.0] - 2026-09-06

The thirteenth audit brief, and the closure of the twelfth. 0.21.0 built the
session lease, the profile coordinator and the startup handshake; this pass
fixes the places that reached past them, and one place where the new code was
never reached at all.

### Fixed

- **The safe delete was dead code.** `DashboardStore` ended up with two
  `delete()` methods, and Python keeps the last one — so the lease-aware
  implementation that 0.21.0 added was never called and every API delete still
  ran the old unleased path. Removed the shadowing pair; there is one delete
  and it reaches the core primitive.

- **Every successful delete leaked the whole browser identity.** `delete()`
  detached the session (a rename to a tombstone) and then called
  `delete_session()`, which detaches *again* — and after the first detach there
  is nothing at that path, so it found nothing and reclaimed nothing. The
  session directory was left on disk as `.deleting-*` forever. It now discards
  the tombstone it actually made, and says exactly which path is left if that
  fails. The rollback no longer stops at its first problem either: every
  restore is attempted, and a failure raises `RollbackFailed` carrying the
  original error and each path that could not be put back.

- **Clearing a value was impossible.** The merge that keeps both profile copies
  in step let an engine-shaped edit win only when its value was *truthy*, so
  `--notes ""` and removing the last tag changed the engine record and left the
  dashboard showing the old one. Field authority is decided by the field now,
  never by the value.

- **Bulk commands still overwrote edits they never saw.** `apply` and
  `pool assign` selected profiles, changed those snapshots and handed the
  finished objects to an atomic write. The write was atomic; the decision was
  not, so an API edit committing in between was acknowledged and then lost. A
  new `patch_many` holds the transaction across re-read → change → name check →
  seal → write.

- **One name contract, on every surface.** `ProfileRepo` refused duplicate
  names on the CLI paths while HTTP create, batch create, rename, bulk update
  and vendor import went straight to the dashboard store — so the API created
  duplicates the CLI refused, and a name-based lookup then silently picked one
  of them. Both surfaces now ask the same function, inside the transaction that
  writes, and `restore --new-id` gives its clone an unclaimed name instead of a
  second profile with the same one. A duplicate answers `409`.

- **A launch reported success before the browser opened.** The handshake
  announced readiness as soon as the child held the lease and had revalidated
  the profile — which is *before* the backend starts. A session that then died
  on a missing browser executable, a backend that would not import or a headful
  launch with no display had already returned 200, registered a running child
  and advanced a schedule. Every backend now announces readiness the moment it
  holds a live browser context and page, and nothing earlier.

- **A launch could open the engine the profile no longer used.** `numera
  launch` resolved the engine and the fingerprint warnings before taking the
  lease, then launched the profile the gate handed back — so a change committed
  in between was honoured everywhere except in the backend actually started.
  Both now come from the profile the gate returns; an explicit `--engine` still
  wins.

- **A schedule could be marked by a browser that was nothing to do with it.**
  The server's child registry was `id -> process`, and the reaper wrote the
  outcome of any exited child into *every* schedule for that profile. Closing
  an ordinary launch rewrote a schedule's result, and one warm-up updated both
  rows of a profile with two schedules. Each child now records what it is and,
  for a scheduled warm-up, which occurrence started it; one finalisation path
  owns the registry, the attached state, `_last_active` and the schedule
  outcome.

- **`/stop` claimed to stop browsers it could not reach.** It returned 200
  unconditionally, but it can only terminate a local child — so when another
  server, desktop or CLI held the session, nothing was stopped and the
  dashboard showed the profile as stopped anyway. It answers `409` for a
  session this server does not own.

- **A failing restore could undo a successful edit.** Restore snapshotted both
  metadata files, staged the session, and on any failure wrote those old bytes
  back — over any edit that had committed in between, since metadata writes do
  not need the session lease. It now stages outside every lock, takes the lease,
  then a short transaction for the swap and the commit, and lets the write batch
  own metadata rollback instead of layering an older snapshot over it.

- **Vault-dependent reads could see half a migration.** The engine store's reads
  joined the root transaction in 0.20.0; the dashboard store's and the proxy
  pool's did not, so a read that decrypts — or that decides whether a secret is
  withheld — could observe records rewritten before the new vault descriptor
  committed. They share the boundary now.

- **`numera ext add` accepted a directory Chromium ignores.** It checked only
  that the path was a directory, while the default-extension and API paths
  require a loadable unpacked extension with a `manifest.json`. All three use
  the same validator.

### Changed

- `POST /api/profiles/{id}/stop` can return `409`. `POST /launch`, `/attach` and
  `/warmup` can return `423` for a locked vault and `404` for a profile deleted
  under them.
- The startup wait is longer, because it now spans a real browser start rather
  than a lease acquisition.

## [0.21.0] - 2026-09-06

The twelfth audit brief, and the closure of the eleventh. 0.20.0 introduced the
session lease and `ProfileRepo`; this pass makes them the *actual* single
authority for a session's lifetime and for a profile's contents, by fixing the
paths that still went round them.

### Fixed

- **A session is nobody's until the process that opens it says so.** The API
  and the scheduler ran a browser by spawning a CLI child and registering it as
  the owner the instant `Popen` returned - but `Popen` proves a process
  started, nothing more. The child took the lease and re-read the vault seconds
  later, so a launch about to exit with `SessionBusy` or `VaultLocked` was
  reported as running; two simultaneous requests spawned two children and the
  loser overwrote the winner's registration, leaving `/stop` pointed at a
  process that owned nothing; and the scheduler wrote `lastRun` for occurrences
  whose browser never opened. There is now a bounded startup handshake
  (`numera/handshake.py`): the child announces itself the moment it holds the
  lease and has revalidated the profile and its credential, and the parent
  waits for that before registering it - terminating and reporting a child that
  never gets there. Per-profile spawn decisions are serialised inside one
  server, so exactly one child can own a profile. A locked vault now answers
  423, a busy session 409, and a deleted profile 404.

- **Opening a session takes the lease before it decides anything.** The gate
  checked the profile and *then* took the lease, so a delete could commit and
  release its own lease in between - and the opener took the free lease and
  launched the snapshot it had already read, recreating `user-data/<id>` for a
  profile that no longer existed. The order is now lease, short root
  transaction, re-read profile and stored credential, release the transaction,
  keep the lease for the browser's lifetime. The gate hands back the profile it
  re-read, so no caller can continue with a stale pre-lease object.

- **A stale dashboard snapshot cannot rewrite the engine copy.** `launch`,
  every probe route, `attach` and the scheduler each read a dashboard record,
  did slow work, and then wrote `to_engine_profile()` of that same snapshot. An
  edit committed in between was reverted on the engine side only, so the two
  copies disagreed and the browser opened an identity the dashboard no longer
  described. They now call `repo.materialize_current(pid)`, which re-reads
  inside the transaction that writes and touches the engine copy only when it
  actually differs.

- **Deleting a profile is one transaction under the lease.** The repository
  deleted the engine record and session in one transaction and unlinked the
  dashboard copy in another, so a failure between them lost the profile while
  leaving its twin, and an API edit landing in the gap recreated the engine
  record - an engine-only profile again. The HTTP path had its own
  implementation that renamed the session directory aside without holding the
  lease at all. There is now one primitive: lease, transaction, detach session,
  both records with byte-level rollback, tombstone reclaimed last. A profile
  another process has open answers 409 and nothing is touched.

- **`restore` and the vendor import write both copies.** A restore committed
  only the engine record, so restoring onto an existing id left the dashboard
  describing the identity it had just replaced. Both records are now written as
  one metadata transaction and rolled back together, with dashboard-only fields
  (folder, notes, trust) preserved across the restore.

- **A schedule claim outlives only a run that actually started.** `record()`
  clears `claimedBy` only alongside `lastRun`, which a failure has no business
  writing - so a materialisation error, a failed spawn or a child that died
  before readiness left the occurrence claimed for the whole 12-hour TTL, and
  every tick in between skipped that schedule without a word. Every failure
  before a confirmed start now releases the claim, reaps any child, and records
  the failure without moving the due time.

- **Two profiles cannot be born with one name.** `bulk create` read the taken
  names, chose a run of free ones, and only then asked for the write - a
  check-then-act two processes both passed, creating distinct ids under
  identical names and leaving `numera <name>` ambiguous with nothing having
  reported a problem. Batch writes now decide names inside the transaction that
  performs them, under an explicit per-caller policy: `bulk create` skips a
  taken name and says how many, `apply` and the vendor import refuse the whole
  batch, and the vendor import reports which names it passed over. A profile
  keeping the name it already has is not a claim, so an unrelated bulk edit
  never fails on a duplicate it did not create.

### Changed

- `numera import-from` returns the profiles it wrote and the names it skipped,
  and prints the skipped ones. Importing the same vendor file twice now adds
  nothing the second time instead of duplicating every profile in it.

## [0.20.0] - 2026-09-06

The eleventh audit brief. Storage was already process-safe; this pass is about
everything that is *not* a JSON write - who owns a browser session, which of
the two profile copies is authoritative, and what a command has already changed
when it reports failure.

### Fixed

- **A browser session has an owner across processes.** `user-data/<id>` is a
  Chromium profile directory and Chromium assumes it owns one exclusively, but
  nothing held it: two servers, a desktop and a CLI could open the same profile
  at once, `backup()` could zip a live one, `restore()` could replace it, and a
  delete's "rename it aside" was never proof, because POSIX renames a directory
  a browser is using without complaint. There is now a per-profile OS-backed
  **session lease**, deliberately unlike a data lock - it refuses immediately
  with `SessionBusy` rather than waiting, and it is never taken while a data
  transaction is held. Launch, attach, `drivers.session()`, warm-up, probes,
  cookies, backup, restore and delete all take it, and cross-process status
  consults it instead of only the local registry.

- **A browser never starts with a credential it could not read.** A locked
  vault hands out `proxy.password = None`, which is indistinguishable from
  "this proxy has no password" - so a launch started with no proxy credential
  at all and browsed from the host's own IP, the one outcome an anti-detect
  profile exists to prevent. Every session open now refuses when the *stored*
  secret cannot be decrypted. A genuinely passwordless proxy is unaffected.

- **The CLI and the dashboard keep one profile, not two that drift.** Dashboard
  writes always mirrored into the engine, but the engine's copy reached the
  dashboard only for an id it had never seen. So `numera edit`, `regen` and
  `apply` left the dashboard's copy stale, and `numera delete` left one behind
  that the next listing adopted straight back - a deleted profile reappearing
  under its own id. `numera/repo.py` is now the one mutation path: read, merge,
  validate, revision, both copies, in one transaction, preserving the fields
  only the dashboard has. The conversion moved to `numera/convert.py` so the
  CLI can use it without the optional HTTP stack.

- **Core CLI edits no longer lose a concurrent change.** `resolve()` → mutate →
  `save()` spans a whole command; the mutation now runs *inside* the
  transaction. Same-name uniqueness moved in with it - `find_by_name()` before
  `save()` was a check two processes could both pass.

- **Slow routes commit onto the current record.** `trust` and `align` held a
  record read before a browser launched and wrote it back tens of seconds
  later, reverting anything edited in between; `is_running()` did the same for
  `_last_active`. All three patch only the fields they own.

- **Proxy assignment is one transaction.** It read every target first and then
  saved in a loop, so a concurrent edit could be overwritten by a stale
  snapshot and a failure on target N left 0..N-1 assigned under a generic
  error. It goes through `patch_many()`.

- **Compound CLI commands are one request.** `config` validates the whole patch
  before writing any of it; `create --engine` commits the default engine in the
  same transaction as the profile; `apply`, `bulk-create` and the vendor
  importer write once through a core `save_many` instead of a loop.

- **A migration is not visible half-done.** `enable_vault` rewrites every secret
  file and commits the descriptor last; a reader outside the transaction could
  see ciphertext while `config.json` still said there was no vault, and hand
  that ciphertext on as the password. Engine `get`, `list` and `resolve` join
  the transaction snapshot - reentrant, so writers pay nothing.

- **First-run import is locked.** Marker check, `copy2()` and marker write were
  unguarded: two fresh desktops could import together, and a file written
  between the importer's "does it exist?" and its copy was overwritten by the
  legacy one. It takes the destination's transaction lock, re-checks inside it,
  and copies each single-file store under that file's own lock.

- **Exactly one worker runs a due schedule.** `due_schedules()` is a pure read,
  so two servers - or a server and `schedule run-due` - could both start the
  same occurrence and both record it. There is an atomic claim with a TTL, and
  a spawn that never started releases it.

- **One warm-up validator.** `minutes=inf` produced a browser with no end time
  and `nan` compared false in both directions; the HTTP route did a bare
  `float()`, and the scheduler had its own copy with a different bound. One
  validator - finite, positive, bounded, known preset, http(s) sites - is used
  by the CLI, the HTTP route, `ScheduleStore`, the scheduler and `warm_up()`
  itself.

- **Crossing OS families resets what the old OS owned.** The reset was gated on
  `os_key != fp.os` *after* `fp.os = os_key`, which can never be true, so a
  profile aligned onto a different OS kept the old version and handset - and
  with them the UA-CH `platformVersion` hint, which is the only thing
  distinguishing Windows 10 from 11 and is read by sites that check. The trust
  grade never saw it. Touch points and device pixel ratio are observed now, and
  fall back to the target OS's own defaults when they are not.

- **`ProxyPool` and `TokenStore` on a `str` root.** Both did `root / "..."` on
  the raw argument.

## [0.19.1] - 2026-09-05

### Changed

- **`probe.py` is a package.** At 839 lines it held four jobs that share
  nothing but a record shape: the checks that need internet (proxy exit,
  WebRTC, TLS), the DNS leak test, the fully offline trust grade, and the
  aligner - the only one of the four that *writes* to a profile. Those are the
  boundaries the file's own section comments already drew, so the split follows
  them: `probe/network.py`, `probe/dns.py`, `probe/trust.py`, `probe/align.py`,
  with `probe/shared.py` for the check shape and the in-memory page, and
  `probe/__init__.py` re-exporting the whole previous surface. Every
  `from numera.probe import ...` in the CLI and the tests is unchanged.

  One coupling had to change with it. `align_to_reality` reads the browser
  through `_observe`, which the alignment tests replace; importing it by name
  would have left them patching a copy nobody calls, and the tests would have
  quietly opened a real browser instead of failing. `align.py` imports the
  module and calls `trust._observe`, and the tests patch it there.

## [0.19.0] - 2026-09-05

The tenth audit brief: the delta the ninth pass left behind. Every item is the
same shape - a decision taken outside the lock that protects the write depending
on it - so the fix is the same shape too: move the decision inside.

### Fixed

- **A vault migration is one cross-process transaction.** `vault enable` and
  `vault change` rewrite every secret-bearing file under the data root and held
  no cross-process lock while they did it. Another process could write a
  plaintext secret after the migration had converted that file and before the
  vault descriptor committed, leaving a store the config calls encrypted with
  one readable secret in it - which no reader can detect and no retry repairs.
  Two simultaneous `vault enable` calls could both observe "disabled" and commit
  different salts, and every secret sealed under the loser's salt was then
  unopenable for good. Both now take the root transaction lock across the state
  check, the enumeration, every write and the verifier commit, and re-check the
  vault state after acquiring it.

- **The vault decision inside `save()` is under the same lock as the write.**
  Reading the descriptor and encrypting happened before the lock was taken, so a
  rotation committing in between meant the save sealed a secret with the salt
  the rotation had just replaced. The record then opened with neither password.
  A writer holding a stale vault identity is refused now.

- **`proxies.json` joins the same lock hierarchy.** Pool writers took the file
  lock alone, which a migration - writing the same file under the transaction
  lock - never waited for.

- **`save_many()` persists the revision it returns.** `_rev` was incremented
  after `batch.write`, so the number reached the client and never the disk: a
  batch create answered `_rev=1` over a file with no revision at all, and a bulk
  update answered N+1 while disk still said N. Every one of those was a revision
  a client could hold and be wrongly rejected - or wrongly accepted - on later.

- **A patch merges under the lock.** `PUT /api/profiles/{id}` read the stored
  record and merged the body onto it before `save()` locked. `_rev` closes the
  lost update for a client that sends a whole profile, but a patch carries no
  revision, so two concurrent patches could merge onto the same old record and
  the second write carried stale values for every field the first had just
  changed. `/bulk` was worse: it built its whole pending list from unlocked
  reads. Both go through `DashboardStore.patch()`/`patch_many()`, whose lock
  covers read, merge, validation, secret preservation, folder resolution,
  revision, mirror and write.

- **`GET /api/profiles` no longer resurrects a deleted profile.** The listing
  writes - it adopts engine-only profiles - and its snapshot was not under the
  transaction lock, so a delete could remove the dashboard copy, the listing
  could observe the engine copy that had not gone yet, and the adoption then
  wrote the profile back.

- **Batch create commits its engine change in the same transaction.** It created
  the profiles and then set the default engine in a second write; a failure
  there returned 500 over profiles already on disk, and the retry duplicated
  every one of them.

- **Config has one lock order.** The batch wrote `config.json` through
  `batch.write`, which takes no file lock, while ordinary settings writes took
  the file lock and no transaction lock - two independent domains, so a rollback
  could restore config over a settings change that had already succeeded. Every
  config mutation now takes the root transaction lock first and the config file
  lock second.

- **A rollback that failed is no longer silent.** `_Batch.rollback()` caught
  `OSError` per file and passed, so a forward failure followed by a rollback
  failure left partial state on disk while the caller saw only the original
  error - indistinguishable from a clean refusal, and the obvious next move,
  retry, wrote on top of it. It raises `RollbackFailed` now, carrying the
  original cause and every file that could not be restored.

- **The dashboard stops inventing a default-engine change.** `save()` ended with
  `if (p.engine) setDefaultEngine(p.engine)`, so editing the notes of a
  Playwright profile under a Camoufox default switched the UI to Playwright over
  a server that had correctly left it alone - and the next new profile then sent
  Playwright explicitly and made the server wrong too. The default is refetched
  from the server after every save.

- **One default-extension validator, enforced by the store.** The rule lived in
  `routes_config.py`, so `numera config --default-extension` wrote straight
  through and `numera ext default-add` checked only `is_dir()`. Both could
  persist a value the API rejects and the launcher silently drops. Only entries
  being *added* are checked, so removing a default whose folder has since been
  deleted still works.

- **A malformed `rotateUrl` already on disk is a refusal, not a 500.**
  Write-time validation cannot reach a value from an older build, a hand-edited
  `proxies.json` or a restored backup, and reading `.port` on
  `http://example.test:99999/rotate` raises outside `rotate()`'s error handling.

- **Named-token uniqueness is decided inside `TokenStore.create()`.** Both
  callers checked for a duplicate name and then called it, so two requests could
  each read a table without the name, both pass, and both append.

- **A force-killed shell closes the browsers too.** The parent watcher leaves
  through `os._exit`, which runs no lifespan teardown, so the engine died and
  every browser it had launched stayed up - orphaned, holding its profile
  directory, invisible to the next launch. The shutdown hooks run on that path
  now.

- **The lock timeout is read per acquisition, not once at import.** As a module
  constant the only way to change it was `importlib.reload(numera.paths)`, which
  a test did - and a reload replaces the module's lock registries and every
  class it defines, so two `_DataLock` objects could guard one path and an
  `except LockTimeout` elsewhere stopped matching the class being raised.

- **`ProxyPool` and `TokenStore` accept a `str` data root.** Both did
  `root / "..."` on the raw argument, so every CLI path that passed a string
  raised `TypeError` before it reached anything worth testing.

### Changed

- **The container image is built and exercised by CI.** It had only ever been
  built by hand. The Dockerfile changes rarely but the engine it installs
  changes on nearly every commit, so a broken `pip install ./engine[...]` would
  have surfaced the first time somebody pulled - which is the same gap that let
  the Linux and macOS portable artifacts stay broken for months. The job builds
  the image, runs it, and checks that it answers `/api/health` with a token,
  refuses without one, round-trips a profile through the real API, and carries
  `cryptography` inside - building alone would prove none of that.

- **The portable artifact is built on every push, not only on a tag.** Three of
  this cycle's defects came out of `build-desktop.yml`, which ran only when
  someone remembered to trigger it. Every push now builds and smokes the
  **Linux** artifact; a tag or a manual run still adds macOS and Windows.

  That split is a measured cost decision, not a compromise on coverage: the
  three desktop jobs take 6.1, 4.6 and 7.1 minutes, and GitHub bills macOS at
  10x and Windows at 2x - roughly 66 billable minutes for the full matrix
  against about 8 for ordinary CI. Linux is the right one to keep, because the
  artifact defects this workflow actually caught were not platform-specific in
  origin; they went unnoticed because nothing ran the workflow at all.

### Added

- **A Windows engine job in CI.** The storage locks are OS locks and Windows
  takes an entirely different path - `msvcrt.locking`, byte-range rather than
  whole-file - which a Linux-only matrix never exercised.

- **A cross-process barrier for concurrency tests** (`engine/tests/_rendezvous.py`).
  Children check in by name and block in `recv()`; the parent releases them one
  at a time. Every new concurrency test states the interleaving it needs and
  fails with an assertion rather than flaking, and none of them contains a
  `sleep`.

## [0.18.0] - 2026-09-05

The ninth audit brief. Every item is about state that could be left inconsistent
by concurrency or by a failure part-way, and every fix is proved by a regression
that fails without it.

### Fixed

- **Storage locks held against threads but not against other processes.**
  `paths._lock_for()` and `transaction_lock()` were `threading.RLock`s in module
  dictionaries. Numera runs several processes against one data root - `numera
  serve`, CLI commands, detached jobs, the scheduler, a second desktop window -
  and none of them could see each other's locks. Two could read, modify and
  atomically replace the same JSON, and the later write erased the earlier one.

  Both now take an OS lock as well (`fcntl.flock` on POSIX, `msvcrt.locking` on
  Windows), released by the OS when the handle closes so a crash cannot wedge
  the data root, with a bounded wait and `NUMERA_LOCK_TIMEOUT` to raise it. The
  regressions use real subprocesses: with the OS layer disabled, four concurrent
  pool adds and four concurrent schedule writes both lose records.

- **A profile update could erase a concurrent one.** The route read the stored
  profile, merged, validated, and only then called `save()`, where the lock was
  taken. Two requests could read the same record and serialise their writes,
  and the second carried stale values for every field it had not touched. The
  store owns read → merge → secret preservation → mirror → write inside the
  lock now, and profiles carry a revision: a client that submits a whole profile
  (the dashboard's inline rename does) gets 409 rather than silently reverting
  somebody else's edit. A patch that names only what it changes still needs no
  revision.

  `_guard_secrets()` and the folder resolution moved inside the lock with it -
  both read the stored record to decide what to preserve, and both could decide
  from a record another writer was already replacing.

- **Profile data and `config.json` were separate commits.** Create and update
  wrote the default engine *before* the profile, so a failed write left the
  global default changed by a request that returned an error; bulk and import
  wrote it *after*, so a failed config write reported failure over profiles that
  had already committed and the obvious retry applied them twice. Both now
  commit in one batch. Failure is injected in both directions.

- **`PUT /api/config` could half-apply a rejected request.** It validated and
  wrote one field at a time, so a request whose third field was invalid had
  already committed the first two. It also used `bool(cfg["headless_launch"])`,
  so the JSON string `"false"` switched headless launching *on*. One validator
  for the whole patch - unknown keys refused, a real boolean required - then one
  atomic write.

- **A failed delete could strand a session.** `restore_session()` swallowed a
  failed rename-back, so when metadata removal failed *and* the session could
  not be put back, the caller saw only the first error while the profile record
  was restored and its real session was left under a `.deleting-*` name nothing
  pointed at. That case raises `SessionStranded` now, naming the tombstone and
  the destination so the repair is deterministic, with the original failure kept
  as its cause. Metadata is restored through the atomic writer rather than a
  truncating `write_bytes`.

- **`ScheduleStore.add()` validated less than `update()` did.** It converted with
  a bare `float()`, and argparse accepts `nan` and `inf` - so `numera schedule
  add --minutes nan` wrote a non-finite number into `schedules.json`, which is
  not valid JSON to a strict reader. It uses the same canonical validator now, so
  validity no longer depends on which surface called the store.

- **The dashboard could show a state the server had rejected.** Five actions
  changed local state before the write was confirmed and only raised a toast on
  failure, and the regular refresh reads *profiles* - so a rejected folder create
  or default-engine change stayed on screen until a reload. The live path is
  server-first, and a failure refetches authoritative config.

- **A force-killed shell could leave the engine running on Linux and macOS.**
  Windows put the engine in a job object; the others had nothing, so the engine
  kept its port and the next launch started a second one against the same
  portable data root. Linux uses `PR_SET_PDEATHSIG` with the parent-race check
  (the parent can die between fork and prctl, and the signal is then already
  spent), and every platform additionally gets a parent watcher inside the
  engine, which is what covers macOS.

- **`rotateUrl` was dropped by structured proxy creation.** `normalize()`
  returned only the core fields, so `POST /api/proxies` discarded a rotation
  endpoint that `PUT` would have kept. It is part of the canonical shape now and
  is validated syntactically at write time - scheme, host, port - without
  requiring DNS. An out-of-range port used to raise from `urlsplit().port`
  outside `rotate()`'s network `try` and surface as a 500; it is a refusal.

### Changed

- `dashboard/package-lock.json` still advertised the old Node range in its root
  entry, `scripts/build-desktop.ps1` still printed `>=20.19 or >=22.12`, and
  `ProxiesView.jsx` still described SOCKS4 as untestable although the relay
  speaks SOCKS4a and health-checks it. All three now match the code.

## [0.17.2] - 2026-09-05

### Fixed

- **An exception could escape the relay's tunnel thread during shutdown.** `selectors.register()`
  raises `ValueError: Invalid file descriptor: -1` - not `OSError` - for a socket that has already
  been closed, and the two `register` calls sat outside `_tunnel`'s `try`. `close()` can pull both
  sockets between the handler opening the upstream and the tunnel registering them, so an ordinary
  shutdown killed the handler thread with an unhandled exception and printed a traceback for it.

  Being a race, it surfaced as an intermittent `PytestUnhandledThreadExceptionWarning` rather than
  a failing test - one run in six on Linux. Promoting that warning to an error is what caught it.
  A direct probe reproduced it on the first attempt before the fix and not once in 200 attempts
  after; eight consecutive Linux suite runs are now clean.

### Changed

- **Vitest 4 -> 5.** None of the APIs its breaking changes touch (`toHaveTextContent`,
  `clearMocks`, `expect.poll`, `sequential`, `vi.mock`) are used here, and the vitest config is a
  single `environment: "jsdom"`; all 17 dashboard tests pass unchanged.

  Vitest 5 requires **Node 22**, so `engines` narrows from `^20.19.0 || >=22.12.0` to `>=22.12.0`
  and the README follows. Every CI job already runs Node 22; claiming support for a version
  `npm test` cannot run on would be worse than dropping it.

- `eslint` 10.10.0, `globals` 17.12.0 and `lucide-react` 1.41.0 - all within their declared
  ranges, so a plain install, no manifest change. The engine's Python dependencies are floors
  (`>=`) rather than pins and needed nothing.

## [0.17.1] - 2026-09-05

### Fixed

- **A profile created while its folder was being renamed resurrected the old folder.**
  `folders.listing()` derives the folder list from the profiles as well as from the meta file, so
  one profile still naming a renamed-away folder brought that name back into the sidebar with a
  single profile stranded in it. The transaction lock serialises the two operations but cannot
  reorder them, so a create landing just after a rename wrote a folder that no longer existed.
  A rename now records where the retired name went, and every profile write follows it - inside
  the transaction lock, because resolving it outside reads meta a rename is about to replace.

- **The proxy relay's tunnel thread could outlive `close()` on Linux, hanging the whole test
  run.** `close()` closes both sockets under the tunnel thread. On Linux, closing a file
  descriptor removes it from the epoll interest list, so no event could ever arrive again - while
  the loop still believed two directions were open and the selector's own map still listed both
  sockets (the map records registrations, not whether the descriptors are still valid). The loop
  had nothing left to wait for and waited anyway; `server_close()` then joined a thread that
  could not finish.

  Windows never showed it: `SelectSelector` raises `OSError` on a closed socket, which the
  handler already caught. So the suite was green on the development machine and hung on every
  Linux runner - as `Engine tests (py3.12)`, which ran 15.3 minutes against a 15-minute job
  ceiling while py3.10 and py3.13 took about one minute. GitHub reports a job killed that way as
  *cancelled*, which is indistinguishable from the `cancel-in-progress` cancellations that
  happened here on the same day.

  The poll is bounded now, and an empty poll checks `fileno()` - the only thing that actually
  answers whether a socket can still deliver. Verified on Linux: the relay suite goes from
  hanging to 20 passed in 4.3s, and the offline suite to 863 passed in 31.6s.

- **The Linux and macOS portable artifacts had never worked.** Both froze
  `engine/numera/__main__.py` as the PyInstaller entry point. PyInstaller runs the entry as
  top-level `__main__` with no package context, so that file's `from .cli import main` raised
  "attempted relative import with no known parent package" and the artifact died on its first
  command. The Windows script had always generated a separate entry with an absolute import,
  which is why only Windows worked - and having two definitions of the same file is how one of
  them stayed wrong. There is now one committed `scripts/engine_entry.py` and both builds freeze
  it, with a test asserting they do.

  It went unnoticed because `build-desktop.yml` had never actually executed: like every other
  workflow before 0.16.0, its runs reported `steps=0` under the account's billing block. This is
  the first genuine run of it, and the Windows job passed on the same run - including the
  clean-checkout dashboard build and the vault round trip added in 0.17.0.

## [0.17.0] - 2026-09-05

### Fixed

- **`pool assign` never checked whether the vault was locked.** The guard was written inside
  the empty-pool branch, after its `return`, so the live path could not reach it. A credentialed
  proxy read from a locked vault arrived with `pass=None` and was written onto every selected
  profile as a proxy with no password, while the changelog said assign refuses while locked. The
  other three verbs guarded correctly; only assign was dead. Its regression drives `cli.main`,
  because a helper-level test of the guard passed the whole time.

- **A locked vault silently ignored a password change.** `encrypt_for_write` returned the stored
  ciphertext when a locked caller supplied a new plaintext, so a proxy or profile password edit
  answered 200 and changed nothing. An empty value still means "keep what is there"; a non-empty
  plaintext is a replacement and is now refused.

- **An envelope this build cannot open was treated as a plaintext password.** `is_encrypted()`
  matched only the currently known prefixes, so a stored `enc:v9:` read back as an ordinary
  string and would have been handed to a launcher as the password itself. The predicate is split:
  `crypto.is_envelope()` (any `enc:v<N>:` marker), `crypto.is_supported_envelope()` (one this
  build knows), and `vault.is_withheld()` now answers by *attempting the read* rather than by
  asking whether the vault verifier opened - so a foreign-key or corrupt record counts as
  withheld even with the vault unlocked, instead of being silently dropped from an export,
  health check or assignment.

- **A failing bulk edit left the default engine changed.** `config.json` is not part of the
  profile batch and was written first, so a storage failure rolled the profiles back and kept a
  default engine chosen by a request that had failed. Config now commits after the data.

- **Bulk update validated less than a single update did.** It ran only `check_fields()`, to which
  `{"engine": "bogus"}` is a perfectly good string, so an unknown engine could reach every
  selected profile and poison the global default. Bulk now runs the same domain validator a
  single-profile PUT does.

- **An import that named an engine got a profile that did not use it.** The explicit choice was
  detected and then discarded - `profile_from_export` carries no engine - so an export naming
  `camoufox` produced a Playwright profile and set the global default to Camoufox.

- **A failed batch could erase a concurrent save.** `save_many` snapshots each file and restores
  the old bytes on failure; a single `save()` landing in that window was overwritten by the
  rollback of a transaction it had nothing to do with. Every profile mutation - engine store,
  dashboard store, folder transaction - now takes one root transaction lock first and its
  per-file lock second. The folder transaction also stopped enumerating profile files before
  locking them, which left a profile created after the snapshot held by nothing.

- **Batch rollback wrote non-atomically.** It restored with a plain `write_bytes`, which
  truncates first, so a crash mid-rollback left an empty file - and an empty `profiles.json`
  reads as "no profiles", the exact failure the atomic writer exists to prevent.

- **A redirected rotate request connected to the wrong host.** `_pinned_opener` closed over the
  first hop's address, so the redirect handler validated hop two and the socket still went to hop
  one. Each request now carries its own validated pin. `urllib.request.ProxyHandler({})` is also
  installed explicitly, because `build_opener()` otherwise adds one built from the environment
  and `HTTP_PROXY` could capture the one request that is direct by design.

- **The Windows build could not run from a clean checkout.** `build-desktop.ps1` aborted unless
  `dashboard/dist/index.html` already existed, and `dashboard/dist` is git-ignored - so the
  Windows CI job, and the README's own instructions, were broken from a fresh clone. The script
  builds the dashboard itself now, and CI asserts no prebuilt `dist` is present.

- **Portable builds shipped without the vault backend.** Both freeze paths installed
  `engine[api,launch]`, and `cryptography` lives in the `secure` extra - so a frozen app could
  not enable the encrypted vault the product advertises, and could not be told to `pip install`
  it into its own embedded runtime. Both now build with `[api,launch,secure]`, and the Windows
  and POSIX artifact smokes prove a real round trip: write a proxy password, enable the vault,
  confirm the on-disk secret is encrypted, reopen it with the master password, and confirm a
  locked read refuses.

- **A rebuild could orphan real profile data.** The portable app writes into `<stage>/data`, and
  only one failure path put that directory back; any other error between moving it out and
  restoring it left profiles under a GUID name in `%TEMP%`. The data is now parked beside the
  build output under a recognisable name and restored on every exit path.

- **The dependency-review check had been red since the day it was added.** It needs the
  dependency-graph API, which on a private repository needs GitHub Advanced Security, so it was
  failing with "not supported on this repository" on every pull request while reviewing nothing.
  It now runs where it can work, and an advisory `npm audit` / `pip-audit` job covers this
  repository meanwhile.

### Added

- **`numera vault change`** - rotate the master password. The error message told users to run it
  and it did not exist, which also meant a master password could never be changed at all. It
  re-encrypts every secret under a new salt through the existing migration, so a failure anywhere
  leaves the old password still working: a half-rotated store would have no password that opens
  all of it, and there is no recovery.

### Changed

- `dashboard/package.json` declares the Node range Vite actually needs
  (`^20.19.0 || >=22.12.0`); the README said 18+, which cannot build the dashboard.
- `engine/README.md` said new vault secrets are written as `enc:v1:`; they have been `enc:v2:`
  since 0.14.0.
- The locked-vault message pointed at `numera vault unlock`, which has never existed. It names
  `NUMERA_PASSWORD` and `--password`, which do.
- `LICENSE` keeps the proprietary terms and the MIT notice persona-studio requires; the
  browser-engine redistribution table moved to `docs/architecture.md`, where it is guidance about
  what may ship rather than a term of the licence.

## [0.16.0] - 2026-09-01

### Fixed

- **An archive member naming a Windows drive was only refused on Windows.** `paths.safe_join()`
  relied on `Path("a") / "C:/x"` evaluating to `C:/x`, which is true on Windows and false
  everywhere else - on Linux the same member is an ordinary relative name, so restoring a
  Windows-made backup quietly created a directory called `C:` inside the profile. Archives are
  portable, so the refusal is now explicit and platform-independent.

- **`test_close_releases_the_port_and_every_thread` asked the wrong question.** It probed by
  rebinding the port, which tests whether the kernel has finished with it rather than whether
  anything is still listening; Linux holds a cleanly closed port in `TIME_WAIT` and the test
  failed on a relay that had shut down correctly. It now probes by connecting, and a refused
  connection means the same thing on both platforms. `SO_REUSEADDR` was rejected as the fix
  because on Windows it permits binding a port that *is* still being listened on, which would
  have made the assertion vacuous on the platform this ships to.

- **`test_the_job_record_never_holds_the_password` asserted against the wrong process.** It kept
  only the most recent `subprocess.Popen` call, which happened to be `spawn`'s on Windows and was
  a stranger's - with no `env=` - on Linux. It now records every call and picks out the one
  running `-m numera` by position.

### Changed

- `dashboard/package-lock.json` carried version `0.12.0` while `package.json` said `0.15.0`;
  the lockfile had not been bumped for three releases and now tracks it again.

### Note

These three are the first defects found by GitHub Actions actually executing. Every workflow added
since 0.9.0 had been written but never run - the account's billing block made each job report
`steps=0` in about three seconds. All three failures were Linux-specific and none was observable
from this machine.

## [0.15.0] - 2026-09-01

### Fixed

- **`--detach` leaked the master password into argv and onto disk.** The password-propagation
  fix missed the direct CLI detach path: `child_argv()` dropped only `--detach`, so
  `numera --password X launch p --detach` handed `--password X` to the child - readable in the
  process table by any local user - and `spawn()` records the argv it ran, so the same secret was
  written into `jobs/<id>.json` in the clear and stayed there. Both flag syntaxes are stripped and
  the unlock reaches the child by environment.
- **A partial `PUT /api/profiles/{id}` replaced the profile instead of patching it.** The body was
  normalised on its own, so every omitted field was treated as "reset me": a name-only rename moved
  a Camoufox profile to **Playwright** (the converter's default), emptied its tags, notes, folder
  and startup URLs, and minted a fresh `_created`, discarding cached trust. The patch is merged onto
  the stored record and the merged result normalised.
- **A failed delete had already deleted the profile.** Metadata went first and the session second,
  so once session removal could fail loudly a held directory produced a failed request whose profile
  was gone. The session is detached by rename first - which fails while nothing else has moved -
  then the records go with rollback, then the detached bytes are reclaimed. A cleanup failure is a
  warning, not a failed delete.
- **Locked-vault proxy work was only half guarded.** `POST /api/proxies/export` rendered blank
  credentials into a file that looked complete, bulk rotate recorded false failures, and CLI `pool
  export/test/rotate/assign` did the same. All refuse while locked. CLI `pool edit` now goes through
  `proxypool.normalize()` like the HTTP route, so an unsupported type or a port of 70000 can no
  longer reach storage from the CLI. A withheld secret no longer dedupes as "no password".
- **The vendor importer rewrote the protocol.** Every `socks*` became SOCKS5 and everything else
  HTTP, so an imported SOCKS4 or HTTPS proxy claimed one protocol while the browser spoke another.
- **`/api/profiles/bulk` and `/api/profiles/import` saved sequentially**, so a failure on item N left
  0..N-1 changed while the request raised - and the obvious retry applied a bulk edit twice or
  duplicated an import. Both commit through the same transaction batch create uses.
- **A folder rename could erase a concurrent profile edit.** `folders` rewrote `dashboard/*.json`
  through its own batch while `DashboardStore.save()` locked per file. The folder transaction now
  takes the profile locks, in sorted order so it cannot deadlock. `ProfileStore.save()` also holds
  its file lock across the read-preserve-write that carries a withheld secret forward.
- **The SSRF guard used an incomplete deny list and re-resolved the host.** `100.64.0.0/10` (carrier
  grade NAT) is neither private nor reserved in Python, so it was allowed; the rule is now
  `is_global`. And validation resolved the hostname once while `urlopen` resolved it again, leaving
  a rebinding window - the connection is pinned to the address that was actually validated, with the
  Host header and TLS hostname verification unchanged, and every redirect hop is revalidated and
  repinned.
- **Bulk delete reported success when it had failed.** The success toast fired unconditionally,
  including after the request threw. It now reports the real `removed` count and surfaces `missing`
  and `failed` separately.
- **The portable app folder was still called `Numera`.** The product, window title and executable
  metadata were renamed to "Numera Browser" earlier, but the folder a user actually unzips was
  missed - along with the line the server prints on startup.

### Changed

- The dashboard sidebar is darker than the content it frames, matching the reference tool; it read
  as raised above the content before. A new `--surface-rail` token carries this; the page floor is
  unchanged, so the theme is not darker overall.
- CI runs `npm run lint`, so the dashboard linter is an actual gate.

## [0.14.0] - 2026-09-01

Finishes the audit set: `plans/011` (deferred from 0.13.0 pending a measurement)
plus the remaining advisory findings.

### Fixed

- **The vault re-read `config.json` twice for every single secret.** `vault()`
  and `unlocked()` each read it, and `decrypt_for_read` calls both - so listing a
  60-proxy pool did over 120 config reads on top of the pool file. This, not
  PBKDF2, was the bulk of the pool's quadratic cost. The config is re-read only
  when its size or mtime changes. Measured on a 60-profile, 60-proxy root with
  the vault on: resolving 60 proxy ids went **7,260 file reads to 60** (1,433 ms
  to 358 ms), updating 20 proxies **6,040 to 20** (1,446 ms to 506 ms), and
  `GET /api/profiles` **240 to 120**. The offline suite dropped from 85 s to 76 s.
- **A proxy pool write read the file twice** - once through `list()` for a
  decrypted view and again to rebuild the map needed to re-encrypt. And a health
  sweep updated each proxy separately, so testing N proxies did N locked
  read-modify-write cycles over the whole file; `update_many()` does one.
- **`GET /api/profiles` listed the store three times** per request, on an
  endpoint the UI polls every three seconds, and `profiles_health` converted
  every profile to engine shape twice per page load.
- **Every authenticated request rewrote `tokens.json`** - an atomic temp write,
  rename and chmod under the lock - to advance `lastUsed` by milliseconds. Only
  written when the stamp actually moves; revocation is still immediate.
- **The dashboard rebuilt its whole stylesheet on every render.** `makeCss()`
  takes no arguments and the theme switches via a `data-theme` attribute, so the
  output never changed.
- **Deleting a selection was one request per profile.** `POST /api/profiles/delete`
  takes a list, and `numera delete` now takes several refs like `pool remove`
  already did - the parity guard caught that the dashboard had gained a
  capability the headless surface could not match.

### Changed

- **The KDF is versioned, and new secrets use 600,000 PBKDF2 rounds** (current
  OWASP guidance) instead of 200,000. The round count is part of what the key was
  derived from, so raising it without a version would have made every existing
  vault unreadable - `enc:v1:` tokens keep opening at 200,000 and only new writes
  are `enc:v2:`. A secret from a future version now says so rather than reporting
  a wrong master password.

## [0.13.0] - 2026-09-01

Two audits worked through together: a full-project advisory pass and a follow-up
brief on the same commit. Plans for every item are in `plans/`.

### Fixed

- **`--password` did nothing for the two surfaces that run longest.** `numera --password X serve`
  unlocked a store and threw it away - `create_app` built its own locked one - so every browser the
  server launched went out with no proxy password. The profile still browsed, just without its
  proxy, which for an anti-detect tool is the worst way to fail. The unlock now reaches the app and
  every child process by **environment, never argv**: a password on a command line is readable by
  any local process. `numera mcp` and its tool subprocesses too.
- **Locked secret-dependent work produced confidently wrong artifacts.** A locked read yields `None`
  by design, and several callers read that as "there is no password": a backup claimed to contain
  the proxy credential and did not, a health check tested a blank password and recorded a real
  failure against a working proxy, and pool assignment wrote `pass=None` over a profile.
  `vault.is_withheld()` tells the two apart, and export, backup, proxy test, rotate and assign now
  refuse rather than emit. Non-secret listing and edits still work while locked.
- **One unreadable record took down the whole listing.** A profile encrypted under a different vault
  key - a restore from another machine, a rotated vault - made `GET /api/profiles` a 500, because
  `unlocked()` only says the current verifier matches, not that this record does. It degrades to
  `None` now, as `vault.decrypt_for_read` already did. `Proxy.from_dict` filters unknown keys like
  `Fingerprint.from_dict`, and the importer tolerates a non-numeric core count instead of aborting a
  half-written import.
- **The vault re-derived its key on every single secret.** 200,000 PBKDF2 rounds for a key that is
  identical every time; listing the pool decrypts every entry and saving it re-encrypts every entry,
  so one "test all" sweep paid it on the order of N-squared. Derived once per password and salt now.
- **SSRF through the proxy change-IP URL.** The one place the engine fetches a caller-supplied
  address from its own network position, checked only for scheme. Private, loopback, link-local and
  metadata destinations are refused, and every redirect hop is re-checked.
- **`--host ""` bound every interface while the guard called it loopback** - exactly the case that
  guard exists to refuse.
- **Unrelated edits changed the global default engine.** A rename, a tag edit, or a bulk note change
  moved it, and in a mixed batch an auto-filled entry overrode an explicit choice. Only an engine
  this request actually named, and actually changed, counts now.
- **Batch create was all-or-nothing on validation but not on storage.** A failing write left the
  earlier profiles on disk while the request reported failure, so the obvious retry made duplicates.
  Both copies of every profile go through one transaction.
- **A deleted session could survive while the API reported it gone.** `rmtree(ignore_errors=True)`
  hid the failure completely; on Windows a running Chromium holds its files. The directory is
  detached by rename first - which fails loudly - and never reported removed when it was not.
- **Folder edits had no lock**, so two concurrent changes each read the same list and the second
  silently dropped the first.
- **Proxy handling had five entry points that disagreed.** Dedupe ignored the protocol while the
  usage count included it; edit bypassed the validator entirely, so a port of `70000` persisted; an
  unsupported text scheme was silently rewritten to HTTP before anything could reject it; and the
  CLI's own scheme map downgraded HTTPS to HTTP. All of them use `normalize()`, `identity()` and
  `server_url()` now - and edit no longer stored the port as an int where create stored a string.
- **Linux and macOS artifacts shipped no dashboard at all.** `--collect-submodules` collects Python
  modules, not `numera/web` package data, so those builds served an API and a blank page. Windows
  was already correct. The POSIX freeze passes `--add-data`, and the smoke test now asserts `/`
  returns the real dashboard and that an asset it references is fetchable.
- **CI never ran the dashboard tests**, so the regressions guarding the auth-lockout and dev-proxy
  fixes could not gate anything.
- **`@pytest.mark.slow` was unregistered** and only three of seven `*_live.py` files carried it - so
  a local `-m slow` run silently missed four, and a typo'd marker would have been accepted as a new
  one. Registered, `--strict-markers` on, and all seven marked: the live suite is 47 tests, not 19.
- Stale vault documentation in `SECURITY.md` and `engine/README.md` that still said the dashboard
  profile store was unencrypted, and a `README.md` install line pinning a wheel version that no
  longer exists.

### Added

- `plans/` - twelve executor-ready implementation plans from the audit, with an index, dependency
  notes, and the findings considered and rejected.
- `numera/vault.py`: `is_withheld()`. `numera/paths.py`: `child_env()`.
  `DashboardStore.save_many()`, `ProfileStore.secret_withheld()`, `ProxyPool.secret_withheld()`.
- Dashboard tests for the Editor's fingerprint-coherence derivations and the bulk generator, and
  ESLint with a `lint` script (17 dashboard tests, up from 5).

## [0.12.0] - 2026-09-01

### Fixed

- **The vault left proxy passwords readable in two of the three places it stores them.** It
  encrypted `profiles/*.json` and stopped there, so `numera vault enable` reported success while
  `dashboard/*.json` and `proxies.json` still held the same passwords in the clear - contradicting
  the product's own claim. `vault.SITES` now lists every secret-bearing file under the data root and
  the migration is all-or-nothing: any failure restores every touched file and leaves the vault off.
  The proxy pool and the dashboard store decrypt on read and encrypt on write like the engine store
  already did, so nothing is encrypted twice and a locked edit keeps the stored value.
  `vault.plaintext_secrets()` exists so the guarantee can be checked rather than asserted.
- **`POST /api/profiles/batch` stored profiles the dashboard cannot render.** It type-checked the
  fields that were present and then saved, so `{"name": "one"}` landed with no `tags` at all while
  the list calls `p.tags.some(...)`. Batch entries go through the same normalisation as a single
  create, which also mints the id before the noise seed is derived from it - two minimal entries
  used to be able to share a fingerprint.
- **Deleting a profile was done twice and rolled back never.** The route deleted the engine record
  and only then found the dashboard had none, so a 404 could arrive after an engine-only profile was
  erased; and the dashboard store swallowed a failed engine delete, leaving the twin behind while
  reporting success. Existence is checked before anything is destroyed, one owner performs both
  deletes with rollback, and the session directory - the only irreversible part - goes last.
- **Settings saved at the same time overwrote each other.** `set_default_engine`,
  `set_default_extensions` and `set_headless_launch` read `config.json` outside the lock and locked
  only the write, so each wrote its own copy of the whole document back. They share one locked
  read-modify-write now (`paths.mutate_json`).
- **A folder rename or delete could stop half way**, leaving some profiles moved and the rest behind
  under a folder list that had already changed. `folders` writes through `paths.write_batch()`,
  which puts every file back if any of them fails, and no longer bypasses the atomic writer.
- **Structured proxy input reached the store unvalidated.** `body.proxies` went straight to the
  pool, so `{"nope": 1}` was a 500, a non-numeric port was stored and only failed at launch, and an
  unsupported type was silently treated as HTTP. Text and structured input share one normaliser
  (`proxypool.normalize`); IPv6, credentials and all four types still round-trip.
- **Minting the first named token locked the dashboard out of its own engine.** Creating it turns
  auth on immediately, and `SettingsView` did not adopt the token before its next request; the 401
  was swallowed, as were the polling loop's and several views'. Every 401 now reports through one
  handler in `api.js` and moves the app to the locked screen, and a token minted while the API was
  open is adopted for the session.
- **`npm run dev` never reached the engine.** `api.js` defaults to same-origin, correct for
  production, but `vite.config.js` had no `/api` proxy - so the documented two-terminal workflow
  called port 5173 and the dashboard came up in demo mode over a healthy engine. The dev server
  proxies `/api` to `127.0.0.1:8787`; the build output and the CORS default are untouched.
- **Linux and macOS release artifacts shipped no browser**, the same defect the Windows build had.
  Both now bundle Chromium (minus the headless shell) and their smoke step launches it through
  `scripts/smoke-portable.sh` instead of only running `--version`, create and list.

### Added

- `numera/vault.py` - the secret-site table, the all-or-nothing migration, and the one
  read/write rule the three stores share.
- `paths.mutate_json()` and `paths.write_batch()` - locked read-modify-write for one file, and
  many-file writes that undo themselves.
- Dashboard tests (vitest + jsdom), and `npm test`.

### Changed

- A locked-vault write raises `crypto.VaultLocked` everywhere. The dashboard store raised
  `PermissionError` for the same condition; nothing caught either, and one type is now used.
- `numera vault enable` reports the number of secrets converted, not profiles - it covers three
  stores now.

## [0.11.1] - 2026-09-01

### Fixed

- **The trust report failed a coherent profile over a renamed timezone.** A profile set to
  `Asia/Kolkata` is reported by the browser as `Asia/Calcutta` - CLDR, which is what Chromium
  answers `Intl.DateTimeFormat().resolvedOptions().timeZone` from, kept the old id. Comparing the
  strings called one zone under two names a contradiction, so every Indian-locale profile lost a
  point it should not have. Both timezone checks (the in-page one and the exit-location one) now
  use `devices.same_timezone()`, which knows the tz-database links. It is not an offset comparison:
  two genuinely different zones can share an offset all year, and treating those as equal would
  hide a real contradiction.

## [0.11.0] - 2026-08-31

### Fixed

- **Every profile edit created a second profile and left the original.** `PUT /api/profiles/{pid}`
  set the id from the path and then called `_validate_profile()`, which minted a fresh UUID
  unconditionally. Renaming a profile gave you two, the old name still there. Create mints; update
  keeps the id from the path and validates it.
- **The API returned a body it would not accept back.** `GET`/`POST` include `status`, `lastActive`
  and the store's private fields, and sending them back was a 422 - so the dashboard's inline
  rename, which posts `{...p, name}` straight from the list, failed silently. Server-owned fields
  are now dropped from input rather than refused; the client's value still never reaches disk.
- **Unknown fields were stored instead of refused.** A typo persisted as a field nothing reads and
  nothing shows. Create, update, batch and bulk now reject unknown keys with a 4xx. The allowlist is
  derived from the converter's own output, so adding a dashboard field carries the contract with it.
- **Windows device names passed the id guard.** `CON`, `NUL`, `COM1` and friends are devices, not
  files - opening one succeeds and reads or writes the device. Server-minted ids never look like
  this; an import, a restore or the CLI could carry one in.
- **Turning the vault on was not atomic.** The descriptor was written first and the profiles
  converted after, so a failure part-way left the vault switched on with plaintext secrets still in
  the profiles it had not reached. Every secret is converted first and the vault becomes visible
  last; a failure puts every profile back.
- **A locked vault refused edits it should have allowed, and allowed a divergence it should have
  refused.** Saving a profile that merely *has* a proxy password failed while locked, because the
  value coming back round is the ciphertext the store handed out - re-writing that downgrades
  nothing. Separately, `DashboardStore.save()` wrote its copy first and swallowed the engine's
  failure, so an edit reported success over a stale engine copy. Both copies now move together or
  neither does.
- **Restore could destroy a live session and update the metadata anyway.** It saved the profile,
  `rmtree(..., ignore_errors=True)`'d the session and moved staging in. On Windows a running
  Chromium holds its files, so the delete failed silently - and `shutil.move` onto a directory that
  still exists moves the source *inside* it. The old session is now renamed aside, which fails
  loudly, and any failure puts all of it back.
- **A job that finished before its parent recorded it lost its outcome.** Parent and child are
  separate processes, so no in-process lock helps: the parent read the record before `Popen`
  returned, the child wrote its outcome, and the parent's write put back a record with none. The
  job showed as running until the startup grace expired, then as "lost", having finished. Identity
  and outcome are separate files now, merged on read.
- **Schedules, config, dashboard profiles and metadata used `write_text`**, which truncates before
  it writes - an interrupted write left a file the store reads as empty. All of them use the atomic
  writer, and `ScheduleStore` re-reads under a lock instead of writing back a stale snapshot.
- **Schedule validation was route-only**, so the CLI and any internal caller wrote `everyHours:
  "soon"` straight to disk. It is enforced at the store now, with a separate `record()` for the
  outcome fields the scheduler itself has to write.
- **SOCKS4 proxies could not be health-checked**, reported as "the protocol has no authentication
  to test". That was the wrong reason: a check needs a connection and an exit IP, neither of which
  involves authentication. The real gap was that the relay spoke only SOCKS5. It speaks SOCKS4a
  now, so the Test button on a SOCKS4 row does what it says, and a SOCKS4 proxy given a *password*
  is refused for the reason that is actually true.
- **The portable build shipped no browser.** `pip install playwright` installs the package; the
  browser is a separate download into a per-user cache, which a clean machine does not have. The
  app started, served the dashboard and failed at the first launch. The build now downloads
  Chromium and ships it beside the engine, and the engine resolves it deterministically. The
  272MB headless shell is left out - the launcher passes `channel="chromium"` precisely to avoid it.

### Added

- `scripts/smoke-portable.ps1` - drives an assembled artifact the way a user's machine would:
  version, create, launch the bundled Chromium, `/api/ready`, and persistence across a restart.
- CI: a Rust job (`cargo fmt --check`, `clippy -D warnings`, `cargo test`), a Windows portable
  build, and an artifact smoke on every platform. Dashboard lint/test steps were **not** added -
  no such scripts exist.

## [0.10.0] - 2026-08-31

### Fixed

- **An iOS profile sent Client Hints, which real Safari never sends.** Chromium adds them itself,
  and 0.9.0 recorded this as a ceiling because nothing removed them. Everything tried sat at one
  layer - `route.continue_(headers=...)`, `--disable-features=UserAgentClientHint,AcceptCHFrame,
  CriticalClientHint` and `--blink-settings=clientHintsEnabled=false` all try to suppress headers
  Chromium has already decided to send, and Playwright's route API rebuilds the set from
  `request.headers()`. Raw CDP `Fetch.continueRequest` replaces the set instead, so a name left out
  never reaches the network. Measured: the server receives no `sec-ch-ua` at all, before or after
  `Accept-CH`. Enabled only for profiles that need it, at the request stage only, so proxy auth is
  untouched and no response body is buffered.
- `fingerprint.sends_client_hints()` now decides this in one place - the headers, the stripping and
  the in-page `navigator.userAgentData` deletion all read it, so the wire and the page cannot
  disagree.

### Removed

- The iOS Client Hints limitation from `docs/architecture.md`. It is no longer true.

## [0.9.0] - 2026-08-31

### Fixed

- **The high-entropy Client Hints went out on every request, including the first.** They were
  pinned as `extra_http_headers` because that is the only header lever that overrides what Chromium
  derives from the host - without it a Windows 11 profile reported the host's OS build. But a header
  applies to every request, and a real Chrome sends those hints only to an origin that has asked
  with `Accept-CH`, so the profile volunteered its architecture and OS build to sites that never
  asked. The launcher now hands Chromium the values as structured metadata over CDP
  (`Emulation.setUserAgentOverride` with `userAgentMetadata`) and lets Chromium's own opt-in logic
  decide when to send them. Measured against a local server: the first request carries the
  low-entropy trio and nothing else, the request after `Accept-CH` carries the profile's values.
  The same metadata answers `navigator.userAgentData.getHighEntropyValues()`, so the page and the
  wire now come from one source instead of two that can drift.
- **The page and the wire disagreed about the CPU on an Apple Silicon Mac.** The in-page hook
  derived architecture from `isMobile` alone and answered `x86` while the header said `arm` - one
  `getHighEntropyValues()` call apart. Both now read the same value.
- `desktop/src-tauri/Cargo.lock` still said `0.6.0`, three releases behind every other manifest.
- A second `## [Unreleased]` heading, inherited from the fork, was presenting long-shipped features
  as pending.

### Changed

- `fingerprint.ua_metadata()` is the single source for the UA-CH values; `client_hint_headers()`
  is its formatted view, so the two cannot drift apart.

### Known limitation

- An iOS profile still sends the low-entropy trio, which real Safari never sends. Chromium adds it
  and nothing removes it; route interception and the relevant `--disable-features` /
  `--blink-settings` flags were all measured against a local server and changed nothing. Recorded
  in `docs/architecture.md`.

## [0.8.0] - 2026-08-31

### Fixed

- **A client could name its own profile id, and the id became a path.** `POST /api/profiles`
  accepted an `id` and both stores joined it onto their directory, so `{"id": "../../escaped"}`
  wrote a file two levels outside the data directory. Creates are always server-minted now, and
  `paths.safe_id()` refuses separators, `..`, drive letters and over-long values at the point an id
  becomes a filename - not only in the route, so import, restore and the CLI are covered by the same
  guard. Lookups treat an unusable id as a miss; writes refuse it.
- **Request bodies were still too permissive.** Bulk accepted arbitrary private fields such as
  `_trust`; a non-object `patch`, a non-list `profiles`, or a non-string import `text` reached
  `.items()`/`json.loads()` and produced a 500. Anything underscore-prefixed is server-owned and
  refused, and malformed shapes are 4xx with nothing written.
- **Import persisted `_new` and could change the default engine.** An imported profile reloaded as a
  create form and saved as a duplicate; and because the importer fills the engine in from the
  current default, `_remember_engine` recorded a choice nobody made. The regression test for this
  posted the wrong body shape (`profiles` instead of `text`), so it passed without importing
  anything - it now asserts the import actually landed.
- **CORS was decided once at startup.** A server that started open kept `allow_origins=["*"]` after
  the first token locked the API. The default is same-origin - the engine serves the dashboard
  itself - with `NUMERA_ALLOWED_ORIGINS` for anything else.
- **The desktop shell could never reach an authenticated engine.** It polled `/api/health`, which is
  behind the token, so a configured token looked like a failed start and the screen that asks for
  one was unreachable. A new public `GET /api/ready` names the service, the version and whether auth
  is required, and nothing else.
- **A locked vault destroyed the password it was protecting.** A locked read yields None for the
  secret (deliberately - the launcher must never authenticate with ciphertext), and an ordinary edit
  carried that None back through `save()` and wrote it over the encrypted value. The stored
  ciphertext is now carried across any save that has no value for it.
- **Persistence was only half atomic.** Profile writes used a direct `write_text`; `TokenStore`
  exposed a lock its own methods never took, so a `verify()` that started before a `revoke()` wrote
  its stale list back and resurrected the revoked token; `ProxyPool.delete()` was outside the pool
  lock. The atomic writer and the per-file lock moved to `paths` and now cover all of them. The
  token file's owner-only permission sat after a `return` and had never run.
- **A read-only portable `data/` directory won on existence alone**, so every write then failed at
  the point of use instead of at the choice.
- **Detached jobs looked recycled on macOS.** The `ps` fallback identity used Python's `hash()`,
  which is randomised per process, so a token recorded by one CLI invocation never matched the next
  and `jobs stop` refused to touch a live job. It is a SHA-256 digest of the normalised line now.
- **Schedules stored whatever they were sent.** `everyHours: "soon"` or a negative `minutes` was
  accepted and surfaced later as a broken next-run time or a zero-work warm-up. One schema validates
  create and update; a too-small interval is still clamped, as `add()` has always done, and
  scheduler-owned fields cannot be patched.
- **One malformed cookie aborted a whole import.** `float()` on a bad expiry raised out of the loop.
  A value that cannot be read becomes a session cookie, and ISO-8601 dates are understood.
- **Proxy protocol could change silently.** The dashboard-to-engine map had no SOCKS4 entry and fell
  back to `http`, so a SOCKS4 profile launched a browser speaking a different protocol; `HTTPS` was
  downgraded the same way. IPv6 broke in both directions (`partition(":")` one way, no brackets the
  other). One canonical `split_server`/`server_url` pair is used by the pool, the dashboard and the
  engine, and an unsupported type raises instead of guessing. Usage counts key on the full identity,
  so credential variants sharing one endpoint no longer inherit each other's count.
- **The portable build froze an engine that could not launch anything.** It installed `engine[api]`,
  leaving out the `launch` extra that carries Playwright - the default engine.


## [0.7.0] - 2026-08-31

### Fixed

- **Client Hints described the host, not the profile.** Chrome adds the high-entropy hints after a
  site sends `Accept-CH`, and it fills them in from the machine it is really running on - so a
  Windows 11 profile started reporting `platform-version "10.0.0"` the moment a site asked. All of
  them now come from the profile, and the wire agrees with `navigator.userAgentData` on the same
  encoding. A two-request live test against a local `Accept-CH` server holds it.
  *Ceiling, measured and recorded:* Chromium cannot be stopped from sending Client Hints at all, so
  an iOS profile still sends them where real Safari sends none. Neither `route.continue_` nor any
  `--disable-features` combination changes it. The values are at least coherent, and
  `navigator.userAgentData` is absent as it should be.
- **Two of the three canvas read paths were unprotected.** `getImageData` and `toBlob` returned the
  real pixels while `toDataURL` returned noised ones - both a bypass and a contradiction, since the
  same drawing hashed differently depending on which call was used. One noise source now backs all
  three, without double-noising; two reads of one canvas still match, and different seeds differ.
- **Font masking only covered the polite question.** It patched `document.fonts.check`, which nobody
  serious uses; the standard probe compares `offsetWidth` against a generic fallback - which is what
  this project's own `probe.py` does, so our own tool could disprove our own control. Width and
  `measureText` probing now see the profile's list.
- **Camoufox inspection wore a different identity from a launch.** `session()` built a shorter
  option set - no `os`, no `locale` - so the trust report, the leak test and the TLS check graded a
  browser that was not the profile, then reported the result as the profile's. Both paths share one
  builder. `geoip` also stopped being unconditional: it moves timezone, locale and geolocation
  together, so forcing it on overrode a profile that had asked for none of that.
- **Imports dropped identity the export was carrying.** The Chrome version in the imported user
  agent was discarded, leaving the UA and `chrome_version` - which builds `sec-ch-ua` - contradicting
  each other. Vendor-specific WebGL paths (`webgl.*`, `graphic.*`, `unmaskedRenderer`) were missed,
  replacing a real GPU string with a local default, and a mobile import got 88px of desktop browser
  chrome subtracted from its viewport.


### Fixed

- **A public IPv6 ICE candidate was never scrubbed.** The WebRTC filter pulled an IPv4 address out
  of a candidate line and treated "no IPv4 here" as safe, so on a v6-capable network the real global
  address was still offered while the v4 one was being stripped. Both families are parsed now, and
  only loopback, link-local, unique-local and mDNS survive.
- **The vault covered one of the two files a profile lives in.** `DashboardStore` wrote the same
  proxy password in plaintext beside the encrypted copy. It now stores ciphertext when the vault is
  unlocked, refuses a *new* secret when it is locked rather than writing it in the clear, and keeps
  the stored value when a non-secret field is edited - which used to blank the password, because the
  editor sends back the redaction it was shown.
- **An IPv6 proxy could not be used at all.** Parsing split on `:`, so `[2001:db8::1]:1080` was
  rejected outright. Bracketed hosts parse, round-trip through export, and are re-bracketed when a
  URL is built for the relay.
- **A failed rotate could be retried immediately.** The cooldown was stamped only on success, so the
  attempt that spent the provider's quota did not count - which is the hammering the cooldown exists
  to prevent.
- **A first-run import gave up permanently on one bad file**, and skipped a whole directory because
  the destination had one entry in it - so a single profile could hide twenty. It merges per entry,
  retries after a failure instead of writing the "done" marker, and now carries the folder layout,
  API tokens and schedules, which had been left behind since they were added.
- **One bad schedule stopped every schedule after it**, silently. Each row is isolated, a warm-up
  that dies on startup is no longer recorded as run, and the exit code of a finished one is stored -
  `Popen` returning means a process started, not that it worked.
- **Browsers survived server shutdown.** The cleanup was registered inside the `if scheduler` branch,
  so a server started without the scheduler left every child running, each holding a lock on a
  profile's user-data directory. It is a lifespan hook now, which also retires the last deprecated
  `on_event` call.
- **A bulk-created Android profile's user agent disagreed with its own fields.** Rebuilding it from
  the OS and Chrome version alone dropped the OS version and handset, substituting the builder's
  defaults. The same omission is fixed in `align`, where a changed OS now moves both with it.


## [0.6.0] - 2026-08-31

### Added

- **Docker.** `docker compose up -d --build` after putting a token in `.env`. The image builds on
  Playwright's own, which already carries Chromium and the ~90 system libraries assembling them onto
  a plain Python image gets wrong on every base-image bump. The port publishes to `127.0.0.1`, not
  every interface; `/data` is a named volume; `shm_size` is raised because Chromium with the default
  64 MB `/dev/shm` dies with an opaque crash. `test_docker.py` holds the image definition against the
  code it packages - a COPY source that stops existing, a browser image that drifts from the
  Playwright client, or a compose file that starts publishing everywhere all fail a test.
- **The product is Numera Browser**, with the real logo in the README, the sidebar, the browser tab
  (there was no favicon at all) and the desktop window and installer. `scripts/make-icons.py`
  regenerates every size from one source, so the intermediates are no longer tracked.

### Fixed

- **API authentication was decided once, at startup.** Minting the first named token protected
  nothing until the process restarted - while the UI said the API now required one. It is read per
  request now, so minting locks the running app and revoking the last one reopens it. The wildcard
  CORS origin is withheld from a named-token-only server too, not just an env-token one: `*` plus a
  bearer token is a token any web page can spend.
- **A non-loopback bind without authentication is refused.** This API creates profiles, launches
  browsers and exports cookies; on `0.0.0.0` anything that can reach the port can use it. The message
  says how to fix it, and `NUMERA_ALLOW_INSECURE_BIND=1` exists for a deployment that already
  authenticates in front.
- **The dashboard turned a 401 into demo mode.** A healthy engine asking for a token was
  indistinguishable from no engine at all, so the app filled the screen with sample profiles that
  were not the user's - which someone could have acted on. It now attaches a bearer token to every
  request, and a locked engine gets a screen that asks for one.
- **Two path-traversal holes, one containment rule.** `ProfileStore.restore()` accepted any archive
  member starting with `user-data/` and joined the rest onto the profile directory, so
  `user-data/../../x` landed two levels up. The Web Store unpacker compared *strings*, so
  `<ext>-evil/x` passed for starting with `<ext>` - the same bug in the icon path. All three go
  through `paths.safe_join` now, which compares resolved paths as paths.
- **A restore could leave a half-made profile.** It saved the profile first and wrote members as it
  went. It stages and validates the whole archive first now, and *replaces* the session rather than
  merging into it - a cookie jar half one identity and half another is worse than either.
- **Every profile created without an explicit seed shared one.** The seed drives canvas, audio and
  ClientRects noise and is derived from the profile id - but it was derived before the store assigned
  one, so they all hashed the literal string `"unsaved"`.
- **A minimal create rewrote the default engine.** Filling the engine in from the current default is
  not a user choice, and persisting it as one let a client's blank field become everyone's default.
- **Bulk and batch writes bypassed validation.** A patch could set `id` - which every profile file,
  session directory and proxy assignment hangs off - or replace a list field with a string, which
  takes the whole profile list down. All three write paths share one field contract now, and a batch
  is validated entirely before any of it is written.
- **Concurrent proxy health checks lost their results.** Eight checks ran in a thread pool, each
  doing an unsynchronised read-modify-write of the same JSON file. Writes are atomic and serialised
  per file now, and a failed write leaves the previous file intact instead of truncating it.
- **A recycled PID could be killed.** Stored PIDs are handed to `taskkill /T /F`, which would take
  down whatever tree now owns that number. A PID is recorded with its process start time, and both
  the liveness check and the terminate refuse a pair that no longer matches.
- **`cookies.write(clear=True, cookies=[])` did nothing.** "Replace this session with nothing"
  returned 0 without opening the browser, leaving the old session in place.
- **The Linux/macOS CI build copied a binary that is never produced** - `numera-desktop` is the
  package name, `numera` is the `[[bin]]` name.
- **The Windows build script only worked on one machine.** It hardcoded a `G:\` Rust install and set
  `RUSTUP_HOME`/`CARGO_HOME` to paths that do not exist elsewhere, which makes rustup start
  installing a toolchain rather than fail. Cargo is resolved from the environment or PATH.


### Changed

- **The dark theme sits on a charcoal now, not near-black.** The floor was `#0a0c11`, which read as
  a hole rather than a surface. Every step of the background ramp moved together, so the separation
  between page, panel, card and raised surface survives instead of flattening. Two values needed
  more than a shift: the faintest text step fell below the contrast floor and was lifted, and the
  mint chip background read as a smudge against the lighter surface. Body text clears 14.6:1 and
  nothing on the page is under 4.5:1. The light theme is untouched.

## [0.5.0] - 2026-08-31

### Added

- **Rotating proxies.** Store a provider's change-IP URL and rotate from the row, in bulk, or with
  `numera pool rotate`. Numera Browser fetches the endpoint, **re-tests the proxy and compares the exit
  IP**: a provider that answers 200 and does nothing is reported as *not moved*, because the
  alternative is a button that lies. A rotate within 20 seconds of the last one is refused, since
  every provider rate-limits this and would hand back the address you already had.
- **All / Errors / Archive on the proxy page.** Archiving retires a proxy without deleting it, so
  which profiles used it and why it was dropped are not lost, and `assign` skips it. The tab counts
  come from the same filter that builds the list, so the number can never disagree with the rows.
  `numera pool archive [--restore]` does the same headlessly.
- **Named API tokens.** `NUMERA_API_TOKEN` is one shared secret: rotating it locks out every client
  at once and nothing records what a token was for. Named tokens are created in Settings or with
  `numera token create <name>`, shown **once**, and stored only as a SHA-256 - there is no file that
  leaking would expose. Revoking one leaves the rest working. The env var stays valid alongside
  them, because it is what the dashboard and any existing script already use.
- **A deterministic avatar per profile.** A list of fifty is read by name, one row
  at a time; a shape is recognised at a glance. Derived from the profile id, so it
  never changes, nothing is stored and no network is involved.
- **Click a profile name to rename it in place.** Renaming used to mean opening the
  editor and saving it back, which re-wrote every field for one word. Enter or blur
  commits, Escape abandons, and the row rolls back if the write fails.
- **Two named switches for Chromium subsystems**: Google services (sync, Safe
  Browsing updates, field trials, usage pings) and the built-in Chromium extensions
  (Web Store connector, Media Router, Cloud Print). Both were already reachable by
  typing a flag; they are switches because "stop Chrome phoning Google" is a thing
  people want, not a flag people remember.

### Fixed

- **Settings said "0 proxies" beside a pool of 27.** The only number the dashboard had was how
  many *profiles were using* a proxy, which is a different question from what is on disk. The
  config endpoint reports the stored count now.
- **The version in the sidebar was a literal `v0.1.0`.** It is the number a user
  actually looks at, and it never read the real one - which is why the app appeared
  stuck at 0.1.0 through three releases. The Settings page had the same fault as a
  fallback, showing a hardcoded version whenever config had not loaded yet.
- **`POST /api/profiles` stored a shape the dashboard could not render.** A body as
  ordinary as `{"name": "alpha"}` passed every check and was saved without `tags`;
  the list calls `p.tags.map(...)`, so one such profile blanked the whole page on
  the next load - exactly what the validator's docstring claimed to prevent, since
  it only checked the types of fields that *were* present. Missing fields are now
  filled from the engine's own defaults, so the row the dashboard draws is the
  identity the browser will actually wear.
- **A profile created through the API got an empty user agent.** Every other field
  in the converter fell back to something coherent; `userAgent` alone fell back to
  `""`. The dashboard always sends one, so only the API path could reach it - and a
  profile presenting an empty UA is a worse tell than any fingerprint mismatch.

- **A desktop build that could not clear its own folder reported success anyway.** `Remove-Item` on
  a file another process holds is a *non-terminating* error, so the script carried on: the copy
  overwrote what it could, every later check still passed because the old files were there, and the
  build said done while shipping the previous version - which is also why the folder had grown from
  32.8 MB to 58.7 MB. Running the app from the build folder is enough to cause it. It now stops,
  says to close Numera Browser first, and **puts the data directory back** before giving up, since that had
  already been moved to a temp folder by then.

## [0.4.0] - 2026-08-31

### Added

- **The profile editor is a full page, not a 580px drawer**, with a two-column body and a
  **profile summary** panel listing every resolved value while you edit - so a contradiction is
  visible at the moment you create it rather than at the next trust check.
- **Six fields the engine had always read but the editor never offered**: client rects, WebGL
  image, media devices, WebGL vendor, speaker count and an editable font list. Every profile had
  been silently taking the default. `test_editor_covers_the_engine.py` now compares what
  `to_engine_profile` consumes against what the dashboard writes, so a field cannot again be
  readable by the engine and unreachable in the UI.
- **Bookmarks.** `Name | URL` per line, written into the profile's own Chromium bookmark file just
  before launch and **merged** - anything bookmarked inside the browser survives, and relaunching
  does not duplicate. An account with an empty bookmark bar reads as new.
- **Extra Chromium arguments**, appended to the flags Numera Browser already sets. They land on the real
  command line; a live test asserts a flag changes what the running page reports.
- **DNS over HTTPS.** Chromium has no plain-nameserver flag, so a well-known resolver address is
  mapped to its DoH endpoint and a custom endpoint can be typed. An address that cannot be honoured
  is **refused at launch** rather than ignored - a leak you believe is closed is worse than a
  failure you can see.
- **Timezone, geolocation and languages can follow the proxy's real exit IP**, resolved at launch
  because the exit country is not knowable before then. A stored timezone is a guess made when the
  profile was created; a clock that disagrees with the IP is the cheapest tell there is.
- **What carries over between launches** is now per-profile: cookies, local storage, IndexedDB,
  open tabs, history, saved passwords, extension data and bookmarks. Unticked stores are cleared on
  the way *in*, so a session that ends in a crash still starts clean. The default is full
  persistence, which is what every existing profile already had.
- **The proxy pool page got the operations it was missing**: multi-select with select-all, bulk
  test, bulk export, bulk delete, an edit dialog, and a masked password with reveal and copy.
  Deleting asks first, and says how many profiles use the proxy.

### Fixed

- **The proxy Test button did nothing for SOCKS proxies** - most of a real pool. `urllib` has no
  SOCKS transport, so the check reported "test it via a profile" instead of testing. It borrows
  `relay.py`, which already speaks SOCKS5, and reports the exit IP and country like any other. A
  credentials failure is now distinguishable from a refused connection.
- **The dashboard 404'd from a source checkout** after `server.py` became a package: the path that
  finds `dist/` was still counting parents from the old depth. Every test passed and every page was
  missing.

### Changed

- `App.jsx` (2463 lines), `cli.py` (1725) and `server.py` (1466) are each now a package of focused
  modules, largest 414. No behaviour change: verified by declaration inventory, bundle size, and a
  before/after screenshot diff inside the same-build noise floor.
- **One version number.** The engine had reached 0.3.0 while the desktop app and the dashboard sat
  at 0.1.0 from the first commit. All five manifests now agree, and a test fails if they drift.

### Added

- **Install an extension from a Chrome Web Store link.** Paste the store URL on the Extensions page
  and the button reads **Install**; `numera ext install <url>` does the same headlessly, with
  `ext installed` and `ext uninstall` beside it. Chromium refuses to install from the store once
  automation is enabled and `--load-extension` only accepts an unpacked directory, so the `.crx` is
  fetched from Google's own update endpoint and unpacked into `<data-dir>/extensions/<id>/` — which
  means it travels with a portable install instead of pointing at one machine. Extension ids are
  exactly 32 letters from `a`–`p`, and a near-miss is diagnosed rather than reported as missing: the
  error names the offending letter, or the actual length, because a truncated copy-paste is the
  common case and "not found" sends you to the wrong place.
- The default-extension list shows each store extension's **name**, read from its manifest, above
  the folder path. A row that reads only a 32-character id says nothing about what is installed.

### Fixed

- **`build-desktop.ps1` deleted the data of the app it was replacing.** The assemble step wiped
  `build/windows/Numera` outright, and the app is *portable* — run it from there and it keeps
  profiles, browser sessions, proxies and logs in `data/` beside the executable. A rebuild therefore
  destroyed real profiles. `data/` is now carried across the rebuild and everything else is
  replaced. `test_build_preserves_data.py` extracts that block from the real script and runs it, so
  a future edit that drops the guard fails a test instead of a user's profiles.

- **The Extensions page accepted things that could never load.** A Chrome Web Store URL, a mistyped
  path or a folder without a `manifest.json` were all stored and reported as saved; the launcher
  then silently dropped them (`_extension_args` keeps only real directories) and nothing loaded,
  with no error anywhere. The API refuses them now, with a message that says what to do instead,
  and rejects the whole write rather than saving part of a list. Verified end to end in a real
  browser: a genuine unpacked extension goes page -> engine -> Chromium and its content script
  runs.
- **API errors lost the server's explanation.** `api.js` threw `PUT /api/config → 400` and
  discarded FastAPI's `detail`, so the dashboard could only ever show a status code. It shows the
  reason now.


- **The Canvas, Audio, Fonts, Geolocation and Do-Not-Track pickers did nothing** (B14). All five
  were rendered by the dashboard and never read by `to_engine_profile()`, so choosing "Real" or
  "Off" changed no browser behaviour at all - the same class as the WebRTC bug fixed in 0.3.0.
  Each is now a real `Fingerprint` field, carried in *both* directions, and enforced where it
  actually matters: canvas honours noise/block/real, audio and font masking can be switched off,
  geolocation distinguishes prompt from allow from block, and DNT reaches both the request header
  and `navigator.doNotTrack`. Proven in a live browser by `tests/test_dead_controls_live.py` -
  reading the mode back out of a dict is not evidence.
- **A fresh install defaulted to an engine it did not have** (B8). `cloak` was hardcoded as the
  default while the installer ships only Playwright, so the first launch click always failed. A
  new install now resolves to an engine that is actually present; an explicit user choice is still
  honoured as-is rather than silently substituted.
- **The engine default could be overwritten by a client's guess** (B7, engine side). Only an
  engine the caller genuinely chose is remembered now - one filled in from the current default is
  not a choice.
- **The API stored profile shapes the dashboard could not render** (B10). `POST`/`PUT` now reject
  a bad shape instead of persisting it and taking the UI down on the next load.
- **One malformed profile blanked the whole app** (B9). There was no error boundary, so a single
  render throw took the React tree down to a white page with no way back. Bad data now degrades to
  a readable panel: per-view (switching tabs clears it), around the profile editor, and once at the
  root as a last resort.
- **An unhandled promise rejection on every engine-offline load** (B11). `api` declared the key
  `health` twice - a safe `async health()` probe that returns false when the engine is down, and
  later `health: () => req(...)` for per-profile data. The second silently shadowed the first, so
  the call that was written to never throw was in fact the throwing one. Renamed to
  `profileHealth`; the probe is a probe again.
- **The profile editor could not use the proxy pool** (B12). Attaching a saved proxy meant retyping
  host, port, user and password by hand. There is a picker now, and manual entry still works.
- **Proxy import collapsed same-country entries** (B13). A label like `IT 1` was reduced to `IT`,
  making four Italian proxies indistinguishable. The country code still aligns the fingerprint; the
  rest is kept as the label.

### Added

- **A portable data root.** Numera Browser now keeps profiles beside the application in `data/`, so the
  folder can be copied to another machine or a flash drive and the profiles travel with it.
  Resolution order is one decision in one place (`paths.py`): explicit `--data-dir`, then
  `NUMERA_DATA_DIR`, then a *writable* directory beside the app, then `~/.numera`, then the
  pre-rename `~/.persona`. The writability check matters: unpacked under Program Files, a data
  directory beside the binary is read-only, and without the check every later write would fail far
  from the cause.
- **First-run import.** An existing install's profiles, proxy pool, config and **browser sessions**
  are copied into the new root the first time it is used. It copies rather than moves, never
  overwrites anything already there, and marks itself done in the destination - so a fresh copy of
  the portable folder imports again, which is what you want when moving machines.
- **Run logging.** A new file per run under `<data-dir>/logs`, UTC to the second, oldest pruned.
  Credentials are redacted by a filter on the *handler*, so a library logging under its own name
  cannot bypass it - verified against the real 16-password proxy pool with zero leaks. `numera
  serve --verbose` raises the detail without weakening redaction.

### Added

- **`build/`, one directory per platform.** Source and artifacts are kept apart: everything under
  `build/` is produced by `scripts/`, never edited by hand, git-ignored, and published through
  GitHub Releases rather than carried in the repository. `build/windows/Numera/numera.exe` is the
  app; `build/server/` holds the wheel.
- **Linux and macOS desktop builds in CI.** `.github/workflows/build-desktop.yml` builds both, plus
  the server wheel, and attaches every platform to the Release for a `v*` tag. They cannot be built
  on Windows: a Tauri binary links against the host's web view.

- **`numera folder list|create|rename|delete`**, with `--json`. Folders were the last capability
  the GUI had and the CLI did not (B18), which made "headless parity" not quite true. The rules
  live in a shared `folders.py` that both the API routes and the CLI call — a rename moves the
  folder's profiles, a delete reassigns rather than destroys them — so the two surfaces cannot
  drift on behaviour subtle enough that the drift would be silent. Parity is now **39/39**.

- **A server / CLI build.** `scripts/build-server.ps1` produces a **platform-independent** wheel
  (`py3-none-any`), so the same file installs on Linux, macOS or Windows. No desktop shell, no
  bundled browser. Verified by installing it into a clean venv and confirming it serves both the
  API and the dashboard.
- **A Linux CI workflow** (`.github/workflows/build-linux.yml`) that builds both the server wheel
  and the Linux desktop app. The desktop half exists because a Tauri binary links against the
  host's WebKitGTK and cannot be cross-compiled from Windows — a Linux runner is the only way this
  project produces one at all.

- **GoLogin-level fingerprint surface.** Twelve more fields carried through *both* conversion
  directions and proven in a live browser: ClientRects and WebGL-image noise as separate switches
  from the vendor/renderer strings, a media-devices mode with speaker count, OS version, Android
  device model, `maxTouchPoints` and `devicePixelRatio`.
- **Android and iOS profile targets**, emulated at the CDP layer — mobile user agent, touch points,
  device pixel ratio, mobile viewport. This is what AdsPower does too, so it is parity rather than
  a shortfall, but the honest limit is recorded in the code: an iOS profile is not running WebKit.

- **A headless parity test.** It reads the API calls `dashboard/src/api.js` actually makes and the
  commands the CLI actually registers, then asserts every GUI capability has a non-GUI path.
  **34 of 37 do.** The three that do not — creating, renaming and deleting folders — are frozen in
  `KNOWN_GAPS` with the reason (folders exist only in the dashboard's profile shape;
  `to_engine_profile()` drops the field, so the engine store has nothing to hang one on. Tags are
  the CLI's equivalent). Closing a gap fails the test too, which is the point: the set cannot drift
  in either direction unnoticed. A separate assertion guards the readers themselves, since a regex
  that quietly matched nothing would make every other assertion pass while proving the opposite.
- **`--json` on the commands an agent would drive**, emitting only machine-readable output on
  stdout with the human text kept off it. The existing human output stays the default.
- **Detached jobs.** `--detach` on a long operation returns a handle immediately;
  `numera jobs list|show|stop|prune` polls and controls them. Verified end to end: spawn, poll a
  running job, stop it, and confirm the OS process is gone.
- **An MCP surface** (`engine/numera/mcp.py`) over the same operations, plus `docs/agents.md`.

- **An Extensions page.** The default set of unpacked Chromium extensions is now managed from the
  sidebar instead of only from the CLI. Adding a folder seeds every profile created afterwards;
  **Apply to all** pushes the set onto profiles that already exist, *merging* rather than replacing
  so a profile keeps whatever it already carried. The page states the limit that surprises people:
  Firefox-family engines cannot load Chromium extensions at all.

- **A portable desktop app.** `scripts/build-desktop.ps1` produces a 32.6 MB folder containing a
  Tauri shell and a frozen engine. Copy it anywhere and double-click: no Python, no Rust, no
  VC++ redistributable, no installer. Only WebView2 is required, which ships with Windows 11.
  The shell picks a free port at runtime, waits for the engine's health endpoint rather than
  sleeping, shows the engine's real stderr if it fails to start, and tears the engine down with
  the window - including when the shell is killed, which is what the job object is for. macOS and
  Linux need their own build hosts; Tauri cannot be cross-compiled from Windows.

- **A local proxy-authentication relay.** A profile whose proxy carries credentials is now
  launched through a small loopback relay that terminates the authentication and forwards
  upstream, so the browser only ever sees an unauthenticated `http://127.0.0.1:<port>`. That is
  the one thing every engine can do: fingerprint-chromium has no password support on
  `--proxy-server`, Firefox cannot do authenticated SOCKS, and Chromium refuses SOCKS
  authentication outright. Stdlib only, ephemeral port, bound to loopback, torn down with the
  browser session. HTTP `CONNECT` and SOCKS5 upstreams are both supported.

### Fixed

- **The CLI and the dashboard kept separate profile stores** (B17). A profile created headlessly
  was invisible in the GUI, and one created in the GUI was invisible to `numera list` until a
  launch happened to materialise it. `DashboardStore` now mirrors every write into the engine
  store, and the profile list adopts anything the CLI created. Mirroring lives in the store rather
  than in each of the five API write paths, so the next one added cannot miss it.

- **`device_scale_factor` ignored the profile and `has_touch` was never set.** The launcher
  hardcoded a 2-or-1 scale factor, so CSS resolution queries disagreed with
  `window.devicePixelRatio`; and without `has_touch`, `(pointer: coarse)` reported desktop while
  `maxTouchPoints` reported a phone. Two contradictions a site can simply read. Both now come from
  the profile.

- **The light theme left a dark page behind it.** `index.html` hardcoded the page background, so
  switching to light repainted the app but not the document beneath it. Both the boot background
  and the app root are tokens now, and follow the theme.

- **A credentialed SOCKS proxy could not be used at all.** Chromium's own refusal
  (`Browser does not support socks5 proxy authentication`) is real; the relay steps around it.
  Confirmed by reproducing the refusal with the relay bypassed and the success with it in place.
- **The proxy/leak test and trust report ran un-proxied for camoufox profiles.** The shared
  `session()` helper opened camoufox with no proxy, so those checks measured the host's own
  connection and then reported it clean - a check that passed by testing the wrong thing. It now
  takes the same proxy path as every other launch.
- `proxy_warnings()` no longer tells the user a SOCKS proxy's credentials are ignored; with the
  relay in front, they are not.
- **The dashboard silently showed fabricated profiles on any port but 8787.** `api.js` baked
  `http://localhost:8787` in at build time, so once the engine started choosing its port the app
  could not reach its own API - and instead of failing, it fell back to demo mode and rendered
  sample data. It is same-origin now, which has no port to get wrong.
- **The log redaction filter dropped any record with a numeric format.** It stringified every
  argument, so a `%d` then raised `TypeError` and `logging` discarded the line. Only strings are
  scrubbed now; a redaction filter that silently deletes log lines is worse than the leak it
  prevents.

### Changed

- **React 18 -> 19, react-dom 18 -> 19, lucide-react 0.383 -> 1.34.** Verified locally, since CI
  cannot run: the app builds, every page opens, the theme toggles, all 63 icons render non-empty
  and the console is clean. The two React packages were upgraded *together* — Dependabot proposed
  them as separate PRs, and taking one without the other leaves a broken pairing.
- **GitHub Actions bumped** to `checkout@v7`, `setup-python@v7`, `setup-node@v7`.
  `dependency-review-action` was left at v4: the proposed `@v5` tag does not exist upstream (only
  `v5.0.0` does), so merging it would have failed the workflow with "action not found".

- **One process, one port.** The engine now serves the built dashboard itself, so
  `numera serve` is the whole application at `http://localhost:8787` - no second dev server, no
  CORS, and one window instead of two. The SPA fallback explicitly refuses `/api/*`: without that
  guard a mistyped endpoint would answer 200 with the HTML page and the dashboard would try to
  parse it as JSON. `start.bat` also stopped guessing - it polls for the port instead of sleeping
  a fixed six seconds and hoping.
- A server or CLI-only install with no `dashboard/dist` still starts normally; the UI routes are
  simply absent.

### Changed - BREAKING

- **The project is now `Numera`.** It was `persona-studio` under MIT. Every user-facing name
  moved with it:

  | | Was | Is |
  |---|---|---|
  | Command | `persona` | `numera` |
  | Python package | `persona` | `numera` |
  | Distribution | `persona-engine` | `numera-engine` |
  | Environment | `PERSONA_API_TOKEN`, `PERSONA_PASSWORD`, `PERSONA_ALLOWED_ORIGINS` | `NUMERA_*` |
  | Data directory | `~/.persona` | `~/.numera` |
  | Launcher marker | `.persona_setup_done` | `.numera_setup_done` |

  Update any script, cron entry, systemd unit or shell profile that calls
  `persona` or exports a `PERSONA_*` variable.

- **Your existing data is not moved and not lost.** `ProfileStore` resolves
  `~/.numera`, and falls back to `~/.persona` when that is the only one present,
  so an existing install keeps reading its profiles, proxies and config in place.
  A real move to a portable application-local root is planned separately.

- **Licence: MIT -> proprietary, all rights reserved.** No redistribution, modification or
  reuse is permitted without written permission - this repository is closed source. The upstream
  MIT notice from persona-studio is still preserved verbatim under **Third-party notices** at the
  end of `LICENSE`, since MIT requires that regardless of the licence wrapped around it; that
  section also records the licence of each browser engine that may be bundled.

- **Python floor raised to 3.10** (was 3.9). patchright and camoufox both require 3.10, so the
  declared floor never matched the engines the project actually launches. `pip install` now
  refuses 3.9 with a clear message; the CI matrix is 3.10/3.12/3.13.

### Removed

- **`start.bat`.** The portable desktop app replaces it: `scripts/build-desktop.ps1` produces a
  folder you copy and double-click, with the engine frozen inside and no Python or Node needed on
  the target machine.


- `CODE_OF_CONDUCT.md` and `CONTRIBUTING.md`.
- `docs/launch.md` - a launch kit for a public open-source release (Show HN, r/selfhosted,
  awesome-lists, Product Hunt). Every channel in it depended on the repo being public, free and
  open source, which it no longer is.

### Documentation

- Every "open source" / GPL claim was swept from the README, badges, comparison table, the
  dashboard footer, the CLI description, `engine/numera/__init__.py`, `docs/social-preview.html`
  and `dashboard/README.md`.
- The `cloak` engine is documented honestly: it is still
  `ProfileStore.DEFAULT_ENGINE`, but **CloakBrowser requires a paid subscription
  and its licence forbids redistribution**, so it cannot be bundled. Camoufox is
  now the recommended engine for anyone who wants a free and open one.

## [0.3.0] - fork

### Fixed
- **WebRTC handed out a non-proxied public IP.** Chromium's
  `--force-webrtc-ip-handling-policy=disable_non_proxied_udp` is passed on every
  proxied launch but does not reliably stop ICE from offering an address that is
  not the proxy's — reproduced with a live proxy: HTTP exit `95.234.60.166` while
  WebRTC offered a different public address. Compounding it, the dashboard's
  "Altered / Disabled / Real" WebRTC picker was cosmetic: the engine had no
  `webrtc` field, so the choice was dropped in conversion and every profile
  behaved identically. The field now exists and reaches the page. `altered`
  (the default) drops candidates carrying a public address from both the ICE
  events and the SDP while leaving private/mDNS ones intact; `disabled` removes
  `RTCPeerConnection`; `real` leaves it alone. Profiles saved before the field
  existed load as `altered`.

### Added
- **`it-IT` and `nl-BE` locales**, with timezones, languages and coordinates, in
  both the engine and the dashboard's own table. A proxy exiting in Italy or
  Belgium had no locale to align to and could never pass the timezone check. A
  test now keeps the four country tables in step so a future locale cannot be
  added to one and forgotten in the others.

## [0.2.0] - fork

First release of the [KiaroSama](https://github.com/KiaroSama/Numera-Browser)
fork of [TechQaiser/persona-studio](https://github.com/TechQaiser/persona-studio).
Everything below is on top of upstream `0.1.0`.

### Fixed
- **Extensions now load in headless mode.** Playwright's default headless binary
  is `chromium_headless_shell`, which cannot load extensions at all, so
  `--load-extension` was silently ignored for every headless launch. Numera Browser now
  launches the full Chromium in `--headless=new` (`channel="chromium"`), which
  loads extensions *and* stops the browser reporting itself as `HeadlessChrome`
  over CDP — a free stealth win, since that string was readable by anything
  attached to the DevTools endpoint.
- **`numera launch --headless` stays up.** It used to open the browser, hit the
  startup URLs and tear everything down immediately, because the "wait for the
  window to close" step only ran in headed mode. A headless session now blocks
  until the browser is stopped, which is what makes the dashboard's launch button
  usable on a machine with no display.
- **Sticky-session proxies survive import.** The pool deduped on `host:port`, so
  a vendor list that points every entry at one gateway and selects the exit node
  from the credentials collapsed into a single proxy. The dedupe key now includes
  the username and password, which is where that identity actually lives.
- **Proxy warnings instead of silent failures.** Chromium implements proxy
  authentication for HTTP(S) only. A SOCKS proxy carrying a username/password is
  now called out at launch and marked failed in the pool health check, rather
  than being accepted and quietly failing to connect.

### Added
- **Global extension set** — `numera ext default-add <folder>`,
  `ext default-list`, `ext default-remove`. Every profile created afterwards
  starts with it, from the CLI, the dashboard or `bulk-create`; exposed over the
  API as `default_extensions` in `GET`/`PUT /api/config`. Chromium has no
  browser-wide extension store to hook, so "global" means copied onto each
  profile at creation — `numera apply --all --extension <folder>` (also new)
  pushes it onto profiles that already exist.
- **Headless launching from the API** — `POST /api/profiles/{id}/launch` accepts
  `{"headless": true}`, and a persisted `headless_launch` config flag makes it
  the default, so the dashboard works unchanged on a headless server.
- **Optional API authentication** — set `NUMERA_API_TOKEN` and every request
  needs `Authorization: Bearer …` (or `X-Numera-Token`). With a token set, CORS
  stops defaulting to `*`; `NUMERA_ALLOWED_ORIGINS` sets it explicitly. Off by
  default, so a localhost install is unchanged.
- **CI and dependency automation** — the workflow now pins job timeouts,
  least-privilege permissions, an oldest/newest Python matrix, `npm ci` from the
  lockfile, and a browser-backed job that proves an extension really loads
  headless. Added Dependabot (pip / npm / actions) and a dependency-review gate.

## [Pre-fork] - inherited

Upstream's unreleased section as it stood at fork time. All of it shipped; the heading is kept for provenance.

### Added
- **Folders manager** (dashboard **Folders** tab): create, rename and delete
  folders, each shown as a card with live profile / running / coherent counts and
  one-click open. Folders are persisted server-side (`GET /api/config` →
  `folders`, plus `POST /api/folders`, `…/folders/rename`, `…/folders/delete`), so
  an empty folder survives a reload; renaming or deleting one moves the profiles
  inside it (delete reassigns to *Unfiled*) rather than dropping anything. The
  editor, bulk-edit and sidebar dropdowns now read this live list.
- **Settings page** (dashboard **Settings** tab): theme toggle, the default launch
  engine for new profiles (persisted via `PUT /api/config`, with an "not
  installed" hint per engine), the on-disk storage location and a data summary
  (profiles / folders / proxies), and an about card. `GET /api/config` now also
  returns `root` and `version`.
- **Command palette** (dashboard, ⌘K / Ctrl-K): fuzzy-search across views,
  actions (new profile, toggle theme) and every profile (open / launch / stop),
  driven entirely from the keyboard (↑/↓/Enter, Esc to close).
- **Light / dark theme** (dashboard): a Sun/Moon toggle in the top bar switches
  the whole app between a new light theme and the original dark one; the choice
  is remembered in `localStorage`. The palette repaints inline styles, icons and
  the full stylesheet in one pass.
- **Toast notifications** (dashboard): success / error / info actions now surface
  as stacked, auto-dismissing toasts instead of blocking `alert()` popups.
- **Headless attach** (`numera attach --headless`, `POST …/attach {headless}`):
  attach a profile's DevTools endpoint without opening a visible window — for
  servers and CI. Exposed as a checkbox on the Automation page.

### Fixed
- **Auto-adjust capped at grade B** — `align` matched everything to the browser
  *except the timezone*, so on an engine that shows the host clock (e.g.
  CloakBrowser) the "timezone matches" check stayed failed forever, holding the
  grade at B (15/16). Align now follows the machine's real timezone and moves
  the locale to the coherent one for that zone (e.g. `Asia/Karachi` → `en-PK`),
  so a fully-aligned profile can reach A. Only moves the timezone when a locale
  models it, so coherence never regresses.
- Coherence's language check now compares the **base** language (`en`), not the
  region (`en-US` vs `en-PK`): an English browser in a Karachi timezone is a
  real, coherent user and shouldn't be flagged. A genuine mismatch (`de` vs
  `en`) is still caught.

### Changed
- Automation attach now reports success as a toast (with the port and whether it
  came up headless).

### Added (earlier this cycle)
- **Automation page** (dashboard, `POST /api/profiles/{id}/attach`): the engine
  already spoke CDP (`numera attach`) — now the dashboard drives it. One click
  opens a profile with a DevTools endpoint and hands back the connection details
  with ready-to-paste **Playwright / Puppeteer / Selenium** snippets (copy
  buttons, language tabs), so your script attaches to the *same* spoofed,
  warmed-up window instead of launching a fresh un-spoofed one. Detach = stop.
  Chromium engines only (cloak / patchright / playwright); the endpoint rejects
  Firefox-based camoufox with a clear message.
- **Fingerprint collision detector** (`numera collisions`, `collision.py`,
  `GET /api/profiles/collisions`): finds profiles that present the *same*
  fingerprint — identical WebGL, screen, hardware and user-agent read as one
  device wearing two hats, so a site can link the accounts and ban them
  together. Pure logic over the stored profiles (no browser), flagged on the
  dashboard **Health** page with a "Shared FP" count and a per-row badge, plus a
  CLI table. `identical` marks the worst case (same canvas/audio seed too).
- **Backup & restore** (`numera backup` / `numera restore`,
  `ProfileStore.backup`/`restore`): zip a profile **and its whole browser
  session** (cookies, localStorage — everything under user-data) into one
  portable file to archive it or move it to another machine. `restore --new-id`
  brings it in as a fresh copy instead of overwriting the original. (Unlike
  `export`/`import`, which move only the config.)
- **Bulk grid actions & tag filtering** (dashboard): the multi-select bar now
  also **warms up** and **exports** the selected profiles in one go (alongside
  the existing launch / edit-all / delete); the filter popover gained a **Tag**
  filter, and tags shown on a profile are clickable to filter by them.
- **Profile health overview** (`GET /api/profiles/health`, dashboard **Health**
  page): one row per profile grading it at a glance — coherence (computed
  instantly from the stored fingerprint), whether a proxy is set and its country,
  the last measured trust grade (cached from the last trust/align run, since a
  live check opens a real browser), any warm-up schedule, and last-active. Rows
  that need attention (incoherent, or a C/D trust grade) are flagged, with
  one-click **trust check** and **auto-adjust** per row.
- **Scheduled warm-up** (`numera schedule`, `scheduler.py`, `/api/schedules*`):
  recurring, unattended warm-up so a session you log into once doesn't go
  "cold". Add a schedule per profile (interval + vertical); when `numera serve`
  is running a background thread fires the due runs itself, and for people who
  don't run the server, `numera schedule run-due` runs whatever's due from cron
  / Windows Task Scheduler. Exposed in the dashboard's Warm-up section as a
  toggle with interval and vertical controls.
- **Warm-up presets per vertical** (`numera warmup --preset`, `warmup.PRESETS`):
  `ads`, `ecommerce`, `crypto`, `social`, `news` (or the default `general`) each
  browse a site set that matches the account's world, so its history reads like
  a real user of that vertical rather than a random walk. Chosen from a dropdown
  in the dashboard; passed through `POST /api/profiles/{id}/warmup`.
- **DNS leak checker** (`numera dns`, `probe.check_dns`,
  `/api/profiles/{id}/dns`): the companion to the proxy test — even with the
  proxy hiding your IP, DNS lookups that go to your own ISP's resolver still
  reveal the sites you visit at your real location. It resolves a batch of
  unique subdomains through the profile, reads back which resolvers were seen
  (and their countries) from a third-party test service, and fails if any sits
  outside the proxy's country. Folded into `proxy test` and surfaced as a **DNS
  leak test** button next to the connection test in the dashboard.
- **Proxy pool — import, test, assign** (`proxypool.py`, `/api/proxies*`):
  a managed pool the dashboard's Proxies page now drives for real. Import a
  pasted list or file in any common layout (`host:port`, `host:port:user:pass`,
  `user:pass@host:port`, `scheme://…`, optional trailing country code),
  health-check each one over HTTP(S) without a browser (exit IP/country/latency),
  and **assign them round-robin across profiles** — which also aligns each
  profile's locale/timezone to its proxy's country. Bulk-created profiles start
  proxy-less and get real proxies from here, instead of the old placeholder
  hosts.
- **Dashboard-side import** (`/api/profiles/import`, `to_dashboard_profile`):
  the GoLogin / AdsPower / Multilogin importer is now wired to an **Import**
  button in the dashboard — pick an export file, and each profile is mapped to a
  coherent identity, saved, and flagged for review if anything didn't line up.
  (Previously the importer lived in the CLI/engine store only.)
- **Auto-adjust — align a profile to reality** (`numera align`, `probe.py`):
  a one-click fix for a low trust grade. It opens the profile, reads back what
  the browser *actually* presents (platform, CPU, memory, GPU, screen, fonts,
  media, languages) and rewrites the fingerprint to match — regenerating the OS
  identity when the engine turns out to render a different OS than the profile
  claimed (e.g. CloakBrowser showing the host's real hardware). Afterwards the
  browser and the profile tell one story, so a detector sees no contradiction;
  the trust score jumps to A. Exposed as `POST /api/profiles/{id}/align` and an
  **Auto-adjust** button next to the trust check, which pulls the rewritten
  values straight back into the editor and shows the before→after grade.
- **Bulk create at scale** (`numera bulk-create`, `bulk.generate_profiles`,
  `POST /api/profiles/batch`): generate thousands of coherent profiles at once —
  up to 10,000 from the dashboard — spread across operating systems, countries/
  locales and Chrome versions, each with its own seed so no two share a
  fingerprint. The dashboard's bulk dialog gained OS/country/browser multi-pick,
  an engine choice, a randomize-hardware toggle and a per-country proxy switch,
  and posts in chunks; the profile grid is now **paginated** so a 10k list stays
  responsive.
- **Real GPUs and displays count as coherent.** `validate()` (and the dashboard
  meter) now accept any WebGL renderer whose graphics API fits the OS (Windows →
  Direct3D, macOS/Linux → desktop OpenGL) and any real-world screen size, not
  just the short curated lists — so a machine-aligned profile carrying its
  actual GPU/resolution isn't wrongly flagged. `navigator.languages` is matched
  on its primary language rather than an exact list.
- **Import from other anti-detect browsers** (`numera import-from`,
  `importers.py`): migrate profiles exported from GoLogin, AdsPower or
  Multilogin. Tolerant field mapping (each value looked up under its known
  aliases), gaps filled from a coherent generated base, and every imported
  profile run through `validate()` so mismatches are flagged for review rather
  than shipped silently.
- **Encrypt-at-rest for secrets** (`numera vault`, `crypto.py`): a master
  password encrypts proxy passwords in the store (Fernet/AES, key via
  PBKDF2-HMAC-SHA256), leaving the rest of each profile readable. Supply the
  password via `NUMERA_PASSWORD` or `--password` at use time so it never hits
  disk; locked stores still list/manage profiles but withhold the secret, and
  saving would-be-plaintext over an encrypted secret is refused. Optional
  `[secure]` extra; degrades gracefully when it isn't installed.
- **TLS/JA3 handshake checker** (`numera tls`, `probe.py`): reads the profile's
  own TLS/HTTP2 handshake back from an echo service and reports its JA3 and JA4
  fingerprints — the network layer JavaScript can never touch, since the server
  sees it before any page script runs. Checks it negotiates TLS 1.3 + HTTP/2 and
  sends GREASE like a real Chromium. Folded into `proxy test` too, plus
  `POST /api/profiles/{id}/tls` and a button in the dashboard.
- **Fingerprint trust checker** (`numera trust`, `probe.py`): opens the profile
  and runs the checks a fingerprinting script runs — webdriver, plugins, UA vs
  platform, WebGL, timezone, font probing, media devices, canvas stability — then
  scores the result. Grades the real browser, where `numera check` grades the
  config. Also `POST /api/profiles/{id}/trust` and a button in the dashboard.
- **Proxy tester + WebRTC leak checker** (`numera proxy test`): routes a real
  request through the profile, reports the exit IP/country/timezone and whether
  they agree with the identity, then checks whether WebRTC exposes an address the
  proxy was meant to hide. Wires up the dashboard's "Test connection" button.
- **Multi-profile synchroniser** (`numera apply`, `bulk.py`): one change applied
  to many profiles, selected by name, `--tag` or `--all`. Unmentioned fields are
  left alone; changing the locale moves timezone and languages with it. Exposed as
  `POST /api/profiles/bulk` and "Edit all" in the dashboard's selection bar.
- **Account warm-up** (`numera warmup`, `warmup.py`): browses ordinary sites at
  human pace so a fresh profile doesn't arrive with an empty history.
- **Local automation API** (`numera attach`, `automation.py`): launches the
  profile with the DevTools protocol open and prints connect snippets, so
  Selenium/Puppeteer/Playwright drive *that* browser — identity, proxy and
  session intact.
- **Extensions manager**: per-profile unpacked extensions (`numera ext
  add|list|remove`, `Profile.extensions`, dashboard field). Only the profile's own
  extensions load, so nothing carries between profiles.
- **More fingerprint signals**: per-OS installed fonts (answered by
  `document.fonts.check`), camera/microphone counts via `enumerateDevices` with
  seeded stable ids, sub-pixel ClientRects jitter, and seeded audio noise. Fonts
  and media counts are covered by `validate()` too.
- **Cookie import/export** (`numera/cookies.py`): move a logged-in session
  between machines or seed a fresh profile with one you already have. Reads
  Cookie-Editor / EditThisCookie JSON, Playwright storage state and Netscape
  `cookies.txt`; writes JSON or `cookies.txt`. Available as
  `numera cookies list|export|import`, `GET`/`POST /api/profiles/{id}/cookies`,
  and Import/Export buttons in the dashboard's **Advanced** tab.
- **One-click Windows launcher** (`start.bat`): double-click to install deps on
  first run, pick the default engine (CloakBrowser first), then start the engine
  API + dashboard and open the browser.
- **Persisted default engine** for new profiles (`config.json`): `numera
  default-engine [name]` command, `GET`/`PUT /api/config` endpoints, and the
  dashboard defaulting new profiles to it. CloakBrowser is the top-priority
  default, and the last engine you pick "wins" (becomes the next default).
- **HTTP API** (`numera serve`, `engine/numera/server.py`) wrapping the store,
  validator and launcher so the dashboard can manage and launch profiles for real.
- **`python -m numera`** entry point (`numera/__main__.py`).
- Dashboard **live/demo mode**: connects to the engine API when available
  (launching opens a real browser), falls back to sample data when it isn't.
- **Pluggable launch engines** (`numera/drivers.py`): choose a stealth backend
  per profile — `playwright` (built-in), `patchright`, `camoufox`, or `cloak`
  (CloakBrowser). New `numera engines` command, `--engine` flag, `Profile.engine`
  field, dashboard engine selector, and `GET /api/engines` endpoint.
- **Dashboard redesign**: new "ink violet" theme, Space Grotesk / Inter / JetBrains
  Mono typography, motion (load reveals, status pulses, drawer slide), and a
  dedicated **Engines** page recommending CloakBrowser & Camoufox with one-click
  install commands. Reduced-motion respected.

### Changed
- **Geolocation now agrees with the proxy.** Launches set
  `navigator.geolocation` to coordinates for the proxy's country (or the
  locale's, without a proxy) and pre-grant the permission, so a site that asks
  gets coordinates that match the exit IP instead of a US timezone with German
  GPS. New `devices.COUNTRY_GEO` / `LOCALE_TO_COUNTRY`.
- **WebRTC IP handling is enforced at launch.** With a proxy, non-proxied UDP is
  disabled (`disable_non_proxied_udp`) so WebRTC can't bypass it; without one,
  only the public interface is exposed so local IPs stay hidden.
- **Hardened the stealth script**: `navigator.webdriver` now reads `false` from the
  prototype (not an own property), `window.chrome`, realistic `navigator.plugins`/
  `mimeTypes` (native `PluginArray`), consistent `permissions.query`, and native
  `toString()` on all patched functions — passing BrowserScan's basic checks.
- `launcher.py` is now a thin dispatcher over `drivers.py`.

### Fixed
- **Canvas and audio noise are now stable across reads.** Both drew from a single
  advancing PRNG and mutated the caller's canvas/buffer in place, so reading the
  same canvas twice gave different bytes — itself a detection signal (CreepJS
  checks for exactly this). Each source now restarts from the profile seed and
  works on a copy.
- **`enumerateDevices` spoofing actually applies.** It was building device objects
  with `Object.assign` against getter-only `MediaDeviceInfo` properties, which
  threw and silently fell back to Chromium's real list. Found by the new trust
  checker.
- **Startup URLs are now opened** on launch (first in the main tab, the rest in
  new tabs) instead of always landing on `about:blank`.
- **Launch button no longer sticks on "running"** after you close the browser
  yourself — the dashboard polls run status in live mode and flips back to start.
- **"Last active" now updates** (server records when a session ends and shows a
  relative time) instead of always reading "never".
- **New profiles no longer overwrite an existing one** — the server assigns each
  profile a unique id, and the local id generator is collision-resistant.
- Suppressed Chromium's infobars: the "controlled by automated test software"
  bar (Playwright) and the cosmetic "--no-sandbox unsupported flag" bar
  (CloakBrowser, via `--test-type` — invisible to websites).
- `numera launch --headless` no longer hangs forever waiting for a window that
  never opens.

## [0.1.0] - 2025

### Added
- **Engine**: coherent fingerprint generator (device-archetype approach) with
  `validate()`; `Fingerprint`/`Proxy`/`Profile` models; JSON-per-profile store
  with import/export and persistent user-data dirs; stealth init-script
  generator; Playwright launcher with proxy support; full CLI.
- **Dashboard**: React web console with profile grid, tabbed fingerprint editor,
  live coherence meter, proxy manager, bulk create, folders, tags, search.
- Monorepo layout, CI, docs, and contribution guides.
