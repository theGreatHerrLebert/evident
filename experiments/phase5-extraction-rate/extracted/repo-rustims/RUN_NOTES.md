# repo-rustims extraction notes

## Workspace-root limitation discovered

The metadata walker (as of this experiment branch) reads
`Cargo.toml` / `pyproject.toml` / `package.json` at the **root**
of the path passed via `--repo`. `rustims` is a Cargo workspace:
the root `Cargo.toml` only contains `[workspace]`, no `[package]`.
Result: **0 metadata claims** at the workspace root.

Fix shipped in **PR #34** (workspace-aware walker) — that PR
descends into Cargo `[workspace].members` and uv
`[tool.uv.workspace].members`, and on a fresh smoke against
rustims produced **20 claims** across 6 workspace members.

For this experiment branch (which is stacked on PR5b + PR5c +
PR5e but NOT PR5d), the workspace gap is visible. The
subpackage extractions below cover what the workspace walker
would surface end-to-end:

- `packages/imspy-core/pyproject.toml` → 3 metadata claims
  (`requires-python: >=3.11,<3.14`, project name, project version)
- `rustms/Cargo.toml` → 3 metadata claims
  (`edition: 2021`, package name, package version)

Subpackage outputs are in sibling directories:
- `extracted/repo-rustims-imspy-core/`
- `extracted/repo-rustims-rustms/`
