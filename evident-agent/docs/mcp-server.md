# `evident-agent-mcp` — exec MCP server

A stdio [MCP](https://modelcontextprotocol.io) server that exposes EVIDENT's
procedure-running logic as four tools. It is the **exec** half of the tool belt; the
**read** half is the Rust `typed-trust-mcp` (query the trust graph). The
`evident-agent drive` launcher wires both for you — this doc is for registering the exec
server in another MCP client, or understanding its contract.

## Launch

```bash
evident-agent-mcp --allow-root <dir-or-file> [--allow-root ...] [--allow-docker] [--allow-extract]
# or, without the console script:
python -m evident_agent.mcp --allow-root <dir> ...
```

Transport is JSON-RPC 2.0 over stdio (one frame per line). All logging goes to **stderr**
— stdout is the frame channel.

| Flag | Effect |
|---|---|
| `--allow-root <path>` | Repeatable allow-list. Tool calls may only touch paths under an allowed root (symlink-resolving; canonicalized at registration and per call). **Required** — with none configured, every call is rejected. |
| `--allow-docker` | Permit `replay` to actually run docker. **Off by default** → `replay` is forced to dry-run. |
| `--allow-extract` | Permit `extract_*` to call the Anthropic API. **Off by default** → `extract_*` runs dry. |

The capability flags are the safety boundary: they live in the server, not the client's
discretion. A client cannot make the server run docker or call the model unless the
operator launched it with the corresponding flag.

## Tools

### `replay`
Runs a measurement claim's docker procedure, scores the artifact, writes the observed
value to the sidecar. The render binary is the server's own trusted `typed-trust` — it is
**not** client-selectable.

Input: `manifest_path` (required), `claim`, `image`, `source_dir`, `budget`, `sidecar`,
`dry_run`, `no_execute`, `render` (`json|md|html|mermaid`).

Output:
```json
{ "sidecar_path": "...|null", "dry_run": false, "capability_gated": false,
  "new_count": 1, "total_count": 3,
  "claims": [{"claim_id":"...","exit_code":0,"duration_s":1.2,"observed":0.0017,
              "skipped_execution":false,"outcome":"completed"}],
  "rendered": "...|null", "log": ["..."] }
```
`outcome` ∈ `completed | timed_out | infrastructure_error | skipped | dry_run`.

### `extract_repo` / `extract_paper`
Draft `tier:research` candidate claims from a repo / paper via a model call.

Input: `repo_path` (or `paper_path`) + `output_dir` (required), plus `project`, `model`,
`dry_run`, `max_tokens`; `extract_paper` also takes `source_id`.

Output:
```json
{ "output_dir": "...", "dry_run": false, "capability_gated": false,
  "source_id": "...", "source_sha": "...", "accepted_claims": 2, "rejections": 1 }
```

### `extract_metadata`
Deterministically extract `metadata_compatibility` claims from `pyproject.toml` /
`Cargo.toml` / `package.json`. No model, no docker — always safe.

Input: `repo` + `output_dir` (required), `project`.

Output:
```json
{ "output_dir": "...", "source_id": "...", "source_sha": "...",
  "emitted_claims": 3, "skipped_files": 0 }
```

## Error model

Two tiers, mirroring `typed-trust-mcp`:

- **Tier-1 — JSON-RPC `error`** (the client cannot recover by retrying the same args):
  `-32602` invalid/missing argument, `-32001` path outside the allow-list, `-32601`
  unknown tool, `-32603` internal defect (sanitized; detail to stderr).
- **Tier-2 — result with `isError: true`** (the model can react): no claim matched a
  filter, docker `infrastructure_error` / timeout, a typed-trust render failure, an
  Anthropic API/SDK failure, or a skipped paper extraction.

A `dry_run: true` / `capability_gated: true` result is **not** an error — it means no
procedure executed because the capability flag was off.

## Register in a client

Use the installed `evident-agent-mcp` console script (after `pip install -e .`). If you
must invoke the module directly, use your interpreter explicitly (`python3 -m
evident_agent.mcp`) — many systems have no bare `python`.

**Claude Code** — `.mcp.json` (or `--mcp-config`):
```json
{ "mcpServers": {
  "evident-exec": {
    "command": "evident-agent-mcp",
    "args": ["--allow-root", "/path/to/corpus"]
  }
}}
```

**Codex** — add the server via per-invocation `-c` overrides (scoped to one session, no
global mutation):
```bash
codex \
  -c 'mcp_servers.evident_exec.command="evident-agent-mcp"' \
  -c 'mcp_servers.evident_exec.args=["--allow-root","/path/to/corpus"]'
```
Avoid `codex mcp add` if you want to keep the wiring scoped to one session — it mutates
`~/.codex/config.toml`. (`evident-agent drive --model codex` uses the `-c` form above.)
Note: interactive `codex` has no `--ignore-user-config`, so this adds the server on top
of your existing config rather than replacing it.

> **Note for headless use:** Codex surfaces MCP tool calls through its approval flow;
> non-interactive `codex exec` has no approver and cancels them (observed). Drive the
> exec server through the interactive TUI.
