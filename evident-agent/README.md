# evident-agent

The agent layer for [EVIDENT](../README.md). It does two jobs:

1. **Populates the trust corpus** — runs cited procedures (`replay`), extracts draft
   claims from repos and papers (`extract*`), and records review events, writing the
   sidecars that the `typed-trust` engine renders into a TrustReport.
2. **Drives an agent over that corpus** — `evident-agent drive` launches a terminal
   agent (Claude Code or Codex) wired to the EVIDENT tool belt, so you can ask *"why
   should I trust this claim?"* and have it answered from evidence, not vibes.

The design principle behind the driver: **MCP is the neutral waist.** Every tool lives
behind an MCP server, so the model runtime is a swappable config detail — the same tool
belt and the same prompt drive either Claude or Codex.

```
  claude  ⟷  codex            pick one at launch (config only)
        │   both are MCP clients
  ┌─────┴──────────────────────────────────────┐
  │  MCP TOOL BELT                               │
  │   typed-trust-mcp   READ — query the graph   │
  │   evident-agent-mcp EXEC — run procedures     │
  └─────┬──────────────────────────────────────┘
        ▼   evident-agent CLI  +  typed-trust engine
```

---

## Prerequisites

| For | You need |
|---|---|
| the CLI + MCP server | Python ≥ 3.10 and `pip` |
| `drive`, `replay --render` | Rust toolchain (`cargo`) to build `typed-trust-mcp` |
| `drive` | an authenticated **Claude Code** (`claude`) and/or **Codex** (`codex`) CLI on `PATH` |
| `replay --allow-docker` | Docker, and the replay image (default `proteon-evident:latest`) |
| `extract` / `extract-paper` (`--allow-extract`) | an `ANTHROPIC_API_KEY` (model call); PDF papers also need `pdftotext` |

`extract-metadata` is deterministic and needs **no** key.

## Install

```bash
# the Python agent — installs the `evident-agent` and `evident-agent-mcp` commands
cd evident-agent
pip install -e .          # use pip3 if pip is not on your PATH

# the Rust read server + engine
cd ../typed-trust
cargo build --release     # produces typed-trust and typed-trust-mcp
```

`drive` finds `typed-trust-mcp` on `PATH` or in the sibling
`typed-trust/target/{release,debug}/` build, and fails with a clear message if it is
missing.

Uninstall with `pip uninstall evident-agent`.

---

> Prefer learning by doing? [`EXAMPLES.md`](EXAMPLES.md) has copy-pasteable, fixture-based
> examples for every workflow below (render a report, replay, extract, review, drive).

## Quick start — drive an agent

A **corpus** is any directory tree containing one or more `evident.yaml` manifests
(`cases/` in this repo is one). Run `drive` from that root — `--root` defaults to the
current directory:

```bash
cd /path/to/your/corpus          # e.g. the evident repo root, which has cases/*/evident.yaml

# default: no docker, no model calls — replay/extract run dry
evident-agent drive --model claude
evident-agent drive --model codex

# enable real execution
evident-agent drive --model claude --allow-docker --allow-extract

# or point at a corpus elsewhere
evident-agent drive --model claude --root /path/to/corpus
```

Then talk to it:

> *"List the claims in this corpus and tell me which have a current verified observation."*
> *"Why should I trust `ball-electrostatic-synthetic-challenge`? Replay it if there's no current observation."*
> *"Draft candidate claims from ./some-repo into ./drafts."*

The agent reads the trust graph first (it is given the corpus root and discovers the
manifests under it), runs procedures only when reading isn't enough, and labels
everything by how it was established (Verified / Judged / Absent / Unknown /
Inconclusive). It will **not** assert that a claim holds without a tool result behind it.

### `drive` options

| Flag | Meaning |
|---|---|
| `--model` | `claude` or `codex` — the only required choice. |
| `--root` | corpus root the servers may access (default: cwd). |
| `--allow-docker` | let `replay` run docker. **Off by default** → replay returns a dry-run. |
| `--allow-extract` | let `extract_*` call the Anthropic API. **Off by default** → extract runs dry. |
| `--driver-prompt` | override the canonical [`EVIDENT_DRIVER.md`](../EVIDENT_DRIVER.md). |

The launcher writes **no persistent global state** (it never runs `claude`/`codex`
config-mutating subcommands). It is composed per invocation in a temp dir that is removed
on exit:

- **Claude** — an ephemeral `--mcp-config` + `--strict-mcp-config` (your user/project MCP
  config is **fully isolated** — it does not leak in) + `--append-system-prompt-file`.
- **Codex** — `codex -C <session> -c 'mcp_servers...'` with the prompt as an ephemeral
  `AGENTS.md`. Codex has no per-invocation config-ignore flag, so our two servers are
  added **on top of** your existing `~/.codex/config.toml` (read, never written). Codex
  runs the **interactive TUI** (not `codex exec`).

### What "default mode" actually means

The capability flags gate the *dangerous* actions, not all writes:

- **No** `--allow-docker` → `replay` runs no container (and, being a dry-run, writes no
  sidecar). **No** `--allow-extract` → `extract`/`extract-paper` make no model call.
- But `extract-metadata` is deterministic and **always writes** its `evident.yaml` +
  `EXTRACTION.md`, and a dry-run `extract` still writes audit files into its
  `output_dir`. "Default mode" means *no docker and no model calls* — not *no writes*.
- Tool calls may only touch paths under `--root` (a symlink-resolving allow-list).
- On Codex, MCP tool calls additionally surface its approval flow before running
  (observed: headless `codex exec` cancels un-approved MCP calls). Treat the server's
  `--allow-docker`/`--allow-extract` as the real boundary; the Codex prompt is a second,
  runtime-dependent gate.

---

## The other commands

| Command | What it does | Writes |
|---|---|---|
| `replay` | Run a measurement claim's docker procedure, score it, record the observed value. `--render` prints the TrustReport. | `<manifest-dir>/last_verified.json` |
| `extract` | Draft `tier:research` claims from a local repo (model call). | `<output-dir>/evident.yaml`, `EXTRACTION.md`, `raw_extraction.json` |
| `extract-metadata` | Deterministic compatibility claims from `pyproject.toml`/`Cargo.toml`/`package.json`. | `<output-dir>/evident.yaml`, `EXTRACTION.md` |
| `review` | Author Endorse / Dissent / Challenge review events. | review-events sidecar |
| `review-extracted` / `promote` / `drop` / `rephrase` | Curate extracted drafts. | manifest / curation log |

Run any with `--help`. `replay` and the `extract*` family are the exact logic the MCP
`replay` / `extract_*` tools wrap — identical behavior whether a human or the driver agent
invokes them.

---

## The MCP servers

- **Read** — `typed-trust-mcp` (Rust, in `../typed-trust`): 11 query tools over the trust
  graph (`list_claims`, `read_report`, `query_observation`, `walk_backing_chain`, …).
- **Exec** — `evident-agent-mcp` (this package): four procedure tools (`replay`,
  `extract_repo`, `extract_paper`, `extract_metadata`).

`drive` wires both for you. To register them in another MCP client, or for the tool
schemas, allow-list, and error model, see [`docs/mcp-server.md`](docs/mcp-server.md).

---

## Troubleshooting

- **`drive: typed-trust-mcp not found`** — build it: `cd ../typed-trust && cargo build --release`.
- **`drive --model codex` does nothing useful headlessly** — Codex routes MCP tool calls
  through an interactive approval prompt, so non-interactive `codex exec` cancels them.
  `drive` runs the interactive TUI; approve the calls there.
- **`claude` says "Invalid API key"** on launch — a stale `ANTHROPIC_API_KEY` in your env
  overrides your Claude subscription credentials. Unset it for the launch:
  `env -u ANTHROPIC_API_KEY evident-agent drive --model claude`.
- **`replay` / `extract` returned a dry-run** — the server lacks `--allow-docker` /
  `--allow-extract`. That's the safe default; pass the flag to enable real execution.

---

## Tests

```bash
pip install -e . && python3 -m pytest
```
