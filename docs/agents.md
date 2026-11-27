# Using Numera Browser without a GUI

In Numera Browser 0.37.0, `numera-engine` runs the Rust CLI, MCP server and Axum HTTP API. Use the CLI for a single command, the HTTP API for the same service the dashboard uses, or MCP for tools that expose input schemas. Both standalone and server modes can use the same data root; do not run two sessions against the same profile.

## Structured CLI output

Put `--data-dir` before the command. Add `--json` after the command and read the exit code as well as the JSON result.

```bash
numera-engine --data-dir ./qa-data create qa-profile --engine playwright --json
numera-engine --data-dir ./qa-data list --json
numera-engine --data-dir ./qa-data check qa-profile --json
```

- `check`, `proxy test`, `tls`, `dns` and `collisions` can return a nonzero code with a valid diagnostic payload. `trust` and `align` can return zero in JSON mode even when the grade is poor. Read the score before deciding what to do.
- JSON mode does not prompt for destructive operations. Use `-y` only after deciding to delete the named profiles. Supply vault credentials securely before a noninteractive secret-dependent operation.
- `list` returns summaries. `show`, profile exports and cookie exports can contain credentials. Keep their output out of shared logs.

Run `numera-engine <command> --help` or see the [CLI reference](../README.md#cli-command-reference).

## Detached jobs and session ownership

```bash
numera-engine --data-dir ./qa-data launch qa-profile --headless --detach --json
numera-engine --data-dir ./qa-data jobs list --json
job_id='replace-with-the-job-id'
numera-engine --data-dir ./qa-data jobs show "$job_id" --lines 20 --json
numera-engine --data-dir ./qa-data jobs stop "$job_id" --json
```

Use the same data root for every command in this sequence. `--detach` returns a handle; it does not mean the browser is ready. Job records and captured output live in `<data-dir>/jobs/`. The `state` field distinguishes `starting`, `running`, `done`, `failed`, `stopped` and `lost`. The `exit` field records the command's exit status when available.

`jobs stop` settles the owned process tree before recording a stop. The profile claim in `<data-dir>/sessions/` can outlive a launcher. If the browser still runs, or Numera cannot prove ownership ended, the profile remains busy. Try `numera-engine stop <ref>` and inspect the logs before taking recovery action. Do not remove a claim merely because its launcher exited or the record is old.

HTTP sessions live in the server's registry. `POST /api/profiles/{pid}/launch` starts a session; use `GET /api/profiles` for its status and `POST /api/profiles/{pid}/stop` to end it. A CLI job handle does not refer to a server session.

## HTTP API

Start the server on loopback and query readiness from a second terminal:

```bash
numera-engine --data-dir ./qa-data serve --port 8787
```

```bash
curl --fail --silent --show-error http://127.0.0.1:8787/api/ready
```

A 0.37.0 server with authentication enabled returns:

```json
{"ok": true, "service": "numera-browser", "version": "0.37.0", "authRequired": true}
```

`/api/ready` does not require a token and does not return profile data. `/api/health` requires authentication when the server is locked. Set `NUMERA_API_TOKEN` in the server environment, or use `numera-engine token create <name>` against the server's data root to mint a named token. Numera shows the token once and stores only its hash. Use `token list` and `token revoke <id>` to manage named credentials. Send them as `Authorization: Bearer <token>`. The HTTP MCP endpoint requires a valid bearer even if the other API routes are unlocked.

The dashboard and API are same-origin by default. Set `NUMERA_ALLOWED_ORIGINS` to list additional browser origins, one per line. CORS does not replace authentication. Keep the API and CDP on loopback unless you have configured a private, authenticated connection.

### Request and error handling

Axum serves the OpenAPI document at `/openapi.json` and its interactive view at `/docs`. Use the schema as a route index; some object fields and application errors have more detail in the Rust handlers and tests under [the engine source tree](../engine-rust/).

- `POST /api/profiles` accepts a minimal body such as `{"name":"qa-profile"}`; the engine fills omitted profile defaults. `PUT /api/profiles/{pid}` uses the ID in the path. The server strips its own bookkeeping fields rather than treating them as user settings.
- Profile writes validate types and reject unknown client fields in Rust. Handle `422` for invalid input, `404` for an unknown profile, `409` for a busy session or conflicting profile state, and `423` when the vault cannot unlock a required secret. A missing or invalid credential returns `401` on protected routes.
- `GET` and `POST /api/profiles/{pid}/cookies` read and import cookies. `GET /api/profiles/{id}/cookies/export` downloads an export. There is no shared cookie jar endpoint.
- The CLI and API share folder and profile storage. Folder deletion reassigns profiles; it does not delete them. Use the profile ID in HTTP paths and a name or ID in the CLI where help says `<ref>`.

## MCP

`numera-engine mcp` speaks newline-delimited JSON-RPC over stdio. To reach an existing authenticated service, run `numera-engine mcp --bridge http://127.0.0.1:8787`. The bridge sends `POST /api/mcp` with a bearer; set `NUMERA_API_TOKEN` in its private environment. A remote bridge requires HTTPS, explicit remote permission and a token. Keep the bridge on loopback for local work.

Call `tools/list` on your connection to see the installed build's tools and input schemas. HTTP MCP runs tools inline and returns cookie exports as content instead of writing a file on the server. The stdio tool set can differ. `numera-engine capabilities --json` lists CLI and MCP capabilities without starting a service.

### Project-local temp and cache

Replace `<numera-dir>` with an absolute writable location, point `command` at the real binary, and create the temp and cache directories first. Omit `.exe` on Linux.

```json
{
  "mcpServers": {
    "numera-browser": {
      "command": "<numera-dir>/bin/numera-engine.exe",
      "args": ["mcp", "--bridge", "http://127.0.0.1:8787"],
      "env": {
        "NUMERA_DATA_DIR": "<numera-dir>/data",
        "NUMERA_LOG_DIR": "<numera-dir>/logs",
        "CLOAKBROWSER_CACHE_DIR": "<numera-dir>/cache/cloak",
        "XDG_CACHE_HOME": "<numera-dir>/cache",
        "TMP": "<numera-dir>/temp",
        "TEMP": "<numera-dir>/temp",
        "TMPDIR": "<numera-dir>/temp"
      }
    }
  }
}
```

For standalone stdio, use `"args": ["mcp"]`. Bridge mode uses the running server's data, logs and browser resources, regardless of the client's local overrides. See the [client configuration notes](../README.md#project-local-mcp-configuration) for Claude Code, Claude Desktop, Codex and Cursor.
