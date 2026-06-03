# repo-rustims extraction notes

## Workspace-root limitation discovered

The metadata walker reads `Cargo.toml` / `pyproject.toml` / `package.json`
at the **root** of the path passed via `--repo`. `rustims` is a Cargo
workspace: the root `Cargo.toml` only contains `[workspace]`, no
`[package]`. Result: **0 metadata claims**.

This is real-world friction the walker should handle eventually. For
the experiment we ran the walker against two subpackages so the
repo is represented:

- `packages/imspy-core/pyproject.toml` → 3 metadata claims
  (`requires-python: >=3.11,<3.14`, project name, project version)
- `rustms/Cargo.toml` → 3 metadata claims
  (`edition: 2021`, package name, package version)

Total surfaced from rustims subpackages: **6 metadata claims**.

## Coverage gap (for the followups doc)

The deterministic walker should descend into Cargo workspace
`members` and `pyproject.toml` `tool.uv.workspace` blocks. Until
then, multi-package repos surface zero claims at the root.
