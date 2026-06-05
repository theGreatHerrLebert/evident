"""Phase 5 PR5b: deterministic metadata extractor.

Reads structural configuration files (pyproject.toml, Cargo.toml,
package.json) and emits ``kind: metadata_compatibility`` claims.

No model call. Metadata is structural — the value declared in the
config file IS the claim. The framework's empirical validator
(source-span + local-binding) doesn't apply here because there's
no value-to-bound comparison; the declaration is the entire
substance.

The walker:
- Reads files via simple parsers (tomllib stdlib for .toml, json
  stdlib for .json)
- Maps well-known fields onto canonical claim shapes
- Skips fields that aren't known compatibility statements (e.g.
  build-system requires, dev dependencies are out of scope for
  this slice)
- Generates stable claim ids derived from the source path

See ``EVIDENT_PHASE5_PAPER_EXTRACTION_DRAFT.md`` and the codex PR5
review which scoped this as a separate path from the empirical
extractor.
"""

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

try:
    import tomllib  # type: ignore
except ImportError:  # pragma: no cover  — Python < 3.11 fallback
    import tomli as tomllib  # type: ignore


@dataclass
class MetadataClaim:
    """One emitted metadata_compatibility claim ready for manifest
    serialization."""

    id: str
    title: str
    claim: str
    metadata_field: str
    declared_value: str
    source_file: str
    source_path: str


@dataclass
class SkippedFile:
    """Why a config file didn't contribute claims.

    Codex F-PR5b-CR2 (P2): distinguish "file exists but corrupt"
    from "file exists but has no recognized fields" so the
    EXTRACTION.md tells the curator whether to investigate.
    """

    path: str
    reason: str  # "no_recognised_fields" | "parse_error"
    detail: Optional[str] = None


@dataclass
class MetadataWalkResult:
    """Bundle of everything one metadata walk produces."""

    source_id: str
    source_sha: str
    claims: list[MetadataClaim]
    skipped_files: list[SkippedFile]
    notes: list[str]


# Strict identifier sanitizer for claim ids: anything that's not
# ASCII alphanumeric, hyphen, or underscore becomes a hyphen.
_ID_BAD_CHARS = re.compile(r"[^A-Za-z0-9_-]+")


def _slug(s: str) -> str:
    cleaned = _ID_BAD_CHARS.sub("-", s.strip().lower())
    cleaned = re.sub(r"-{2,}", "-", cleaned)
    return cleaned.strip("-")


# ---------------------------------------------------------------------
# Per-file extractors
# ---------------------------------------------------------------------


def _extract_pyproject(
    path: Path, repo_slug: str, source_file_label: str = "pyproject.toml",
) -> "list[MetadataClaim] | tuple[None, str]":
    """Read a pyproject.toml and emit metadata claims for the
    well-known compatibility fields.

    Returns either a list of claims OR ``(None, detail)`` indicating
    a parse error (codex F-PR5b-CR2 P2 distinguishes parse failures
    from no-recognized-fields).

    Emitted claims:
    - ``python_version_requirement`` from ``project.requires-python``
    - ``project_name`` from ``project.name``
    - ``project_version`` from ``project.version``
    """
    try:
        with path.open("rb") as f:
            doc = tomllib.load(f)
    except tomllib.TOMLDecodeError as exc:
        return (None, f"TOML parse error: {exc}")
    except OSError as exc:
        return (None, f"OS error: {exc}")
    project = doc.get("project") or {}
    out: list[MetadataClaim] = []
    pyreq = project.get("requires-python")
    if isinstance(pyreq, str) and pyreq.strip():
        out.append(
            MetadataClaim(
                id=f"{repo_slug}-pyproject-requires-python",
                title=(
                    f"{repo_slug} requires Python {pyreq.strip()}"
                ),
                claim=(
                    f"pyproject.toml declares "
                    f"requires-python = {pyreq.strip()!r}"
                ),
                metadata_field="python_version_requirement",
                declared_value=pyreq.strip(),
                source_file=source_file_label,
                source_path="project.requires-python",
            )
        )
    name = project.get("name")
    if isinstance(name, str) and name.strip():
        out.append(
            MetadataClaim(
                id=f"{repo_slug}-pyproject-name",
                title=f"{repo_slug} declares Python package name",
                claim=(
                    f"pyproject.toml declares "
                    f"project.name = {name.strip()!r}"
                ),
                metadata_field="project_name",
                declared_value=name.strip(),
                source_file=source_file_label,
                source_path="project.name",
            )
        )
    version = project.get("version")
    if isinstance(version, str) and version.strip():
        out.append(
            MetadataClaim(
                id=f"{repo_slug}-pyproject-version",
                title=f"{repo_slug} declares Python package version",
                claim=(
                    f"pyproject.toml declares "
                    f"project.version = {version.strip()!r}"
                ),
                metadata_field="project_version",
                declared_value=version.strip(),
                source_file=source_file_label,
                source_path="project.version",
            )
        )
    return out


def _extract_cargo_toml(
    path: Path, repo_slug: str, source_file_label: str = "Cargo.toml",
) -> "list[MetadataClaim] | tuple[None, str]":
    """Read a Cargo.toml and emit metadata claims for well-known
    fields: ``package.rust-version`` (MSRV), ``package.name``,
    ``package.version``, ``package.edition``.

    Returns ``(None, detail)`` on parse error (codex F-PR5b-CR2 P2).
    """
    try:
        with path.open("rb") as f:
            doc = tomllib.load(f)
    except tomllib.TOMLDecodeError as exc:
        return (None, f"TOML parse error: {exc}")
    except OSError as exc:
        return (None, f"OS error: {exc}")
    package = doc.get("package") or {}
    out: list[MetadataClaim] = []
    rust_version = package.get("rust-version")
    if isinstance(rust_version, str) and rust_version.strip():
        out.append(
            MetadataClaim(
                id=f"{repo_slug}-cargo-rust-msrv",
                title=(
                    f"{repo_slug} requires Rust MSRV "
                    f"{rust_version.strip()}+"
                ),
                claim=(
                    f"Cargo.toml declares "
                    f"package.rust-version = {rust_version.strip()!r}"
                ),
                metadata_field="rust_msrv",
                declared_value=rust_version.strip(),
                source_file=source_file_label,
                source_path="package.rust-version",
            )
        )
    edition = package.get("edition")
    if isinstance(edition, str) and edition.strip():
        out.append(
            MetadataClaim(
                id=f"{repo_slug}-cargo-edition",
                title=f"{repo_slug} declares Rust edition",
                claim=(
                    f"Cargo.toml declares "
                    f"package.edition = {edition.strip()!r}"
                ),
                metadata_field="rust_edition",
                declared_value=edition.strip(),
                source_file=source_file_label,
                source_path="package.edition",
            )
        )
    name = package.get("name")
    if isinstance(name, str) and name.strip():
        out.append(
            MetadataClaim(
                id=f"{repo_slug}-cargo-name",
                title=f"{repo_slug} declares Cargo package name",
                claim=(
                    f"Cargo.toml declares "
                    f"package.name = {name.strip()!r}"
                ),
                metadata_field="cargo_package_name",
                declared_value=name.strip(),
                source_file=source_file_label,
                source_path="package.name",
            )
        )
    version = package.get("version")
    if isinstance(version, str) and version.strip():
        out.append(
            MetadataClaim(
                id=f"{repo_slug}-cargo-version",
                title=f"{repo_slug} declares Cargo package version",
                claim=(
                    f"Cargo.toml declares "
                    f"package.version = {version.strip()!r}"
                ),
                metadata_field="cargo_package_version",
                declared_value=version.strip(),
                source_file=source_file_label,
                source_path="package.version",
            )
        )
    return out


def _extract_package_json(
    path: Path, repo_slug: str, source_file_label: str = "package.json",
) -> "list[MetadataClaim] | tuple[None, str]":
    """Read a package.json and emit metadata claims for
    ``name``, ``version``, ``engines.node``.

    Returns ``(None, detail)`` on parse error (codex F-PR5b-CR2 P2).
    """
    try:
        with path.open(encoding="utf-8") as f:
            doc = json.load(f)
    except json.JSONDecodeError as exc:
        return (None, f"JSON parse error: {exc}")
    except OSError as exc:
        return (None, f"OS error: {exc}")
    if not isinstance(doc, dict):
        return (None, "JSON top-level is not an object")
    out: list[MetadataClaim] = []
    name = doc.get("name")
    if isinstance(name, str) and name.strip():
        out.append(
            MetadataClaim(
                id=f"{repo_slug}-pkgjson-name",
                title=f"{repo_slug} declares npm package name",
                claim=(
                    f"package.json declares name = {name.strip()!r}"
                ),
                metadata_field="npm_package_name",
                declared_value=name.strip(),
                source_file=source_file_label,
                source_path="name",
            )
        )
    version = doc.get("version")
    if isinstance(version, str) and version.strip():
        out.append(
            MetadataClaim(
                id=f"{repo_slug}-pkgjson-version",
                title=f"{repo_slug} declares npm package version",
                claim=(
                    f"package.json declares "
                    f"version = {version.strip()!r}"
                ),
                metadata_field="npm_package_version",
                declared_value=version.strip(),
                source_file=source_file_label,
                source_path="version",
            )
        )
    engines = doc.get("engines")
    if isinstance(engines, dict):
        node = engines.get("node")
        if isinstance(node, str) and node.strip():
            out.append(
                MetadataClaim(
                    id=f"{repo_slug}-pkgjson-engines-node",
                    title=f"{repo_slug} requires Node {node.strip()}",
                    claim=(
                        f"package.json declares "
                        f"engines.node = {node.strip()!r}"
                    ),
                    metadata_field="node_version_requirement",
                    declared_value=node.strip(),
                    source_file=source_file_label,
                    source_path="engines.node",
                )
            )
    return out


# ---------------------------------------------------------------------
# Workspace detection
# ---------------------------------------------------------------------


def _detect_workspace_members(repo_path: Path) -> list[str]:
    """Detect Cargo / uv-workspace member directories at the repo
    root. Returns a deduplicated, sorted list of POSIX-style relative
    paths.

    Two sources today:
    - ``Cargo.toml`` ``[workspace].members`` (Cargo workspaces).
      Members can be glob patterns (``crates/*``).
    - ``pyproject.toml`` ``[tool.uv.workspace].members`` (uv
      workspaces). Same glob semantics.

    Returns an empty list when neither workspace marker is present;
    callers fall back to root-only extraction. Glob expansion uses
    pathlib so a missing intermediate directory or a non-matching
    pattern silently contributes nothing rather than failing the
    walk.
    """
    members: set[str] = set()
    cargo_root = repo_path / "Cargo.toml"
    if cargo_root.is_file():
        try:
            with cargo_root.open("rb") as f:
                doc = tomllib.load(f)
        except (tomllib.TOMLDecodeError, OSError):
            doc = {}
        ws = doc.get("workspace") or {}
        cargo_members = ws.get("members")
        if isinstance(cargo_members, list):
            for entry in cargo_members:
                if isinstance(entry, str):
                    members.update(_expand_member_glob(repo_path, entry))
    pyproject_root = repo_path / "pyproject.toml"
    if pyproject_root.is_file():
        try:
            with pyproject_root.open("rb") as f:
                pdoc = tomllib.load(f)
        except (tomllib.TOMLDecodeError, OSError):
            pdoc = {}
        uv_ws = (
            (pdoc.get("tool") or {})
            .get("uv", {})
            .get("workspace", {})
        )
        uv_members = uv_ws.get("members")
        if isinstance(uv_members, list):
            for entry in uv_members:
                if isinstance(entry, str):
                    members.update(_expand_member_glob(repo_path, entry))
    return sorted(members)


def _expand_member_glob(root: Path, pattern: str) -> list[str]:
    """Expand a Cargo/uv member entry into concrete repo-relative
    directory paths. Plain entries pass through unchanged; glob
    entries are expanded via ``Path.glob``.
    """
    pattern = pattern.strip()
    if not pattern:
        return []
    if any(c in pattern for c in "*?["):
        out: list[str] = []
        for match in root.glob(pattern):
            if match.is_dir():
                try:
                    out.append(match.relative_to(root).as_posix())
                except ValueError:
                    continue
        return out
    candidate = (root / pattern).resolve()
    try:
        rel = candidate.relative_to(root.resolve()).as_posix()
    except ValueError:
        return []
    return [rel] if candidate.is_dir() else []


# ---------------------------------------------------------------------
# Public walker entry point
# ---------------------------------------------------------------------


def _slug_prefix_from_source_id(source_id: str, fallback: str) -> str:
    """Codex F-PR5b-CR3 (P2/P3): derive the claim id prefix from
    the resolved source_id (e.g. ``github:owner/repo@<sha>`` →
    ``owner-repo``) so two repos that share a basename (``src``,
    ``repo``, ``evident``) don't collide.

    Strips the ``@<sha>`` suffix. Falls back to ``fallback``
    (typically the repo basename slug) if the source_id has no
    structured owner/repo prefix.
    """
    base = source_id.split("@", 1)[0]
    if base.startswith("github:"):
        path = base[len("github:"):]  # owner/repo
        return _slug(path.replace("/", "-"))
    if base.startswith("local:"):
        # local:<basename> — same as fallback.
        return _slug(base[len("local:"):])
    return fallback


def walk_repo_metadata(
    repo_path: Path,
    *,
    source_id: Optional[str] = None,
) -> MetadataWalkResult:
    """Walk ``repo_path`` looking for pyproject.toml, Cargo.toml,
    and package.json at the repo root. Returns a
    ``MetadataWalkResult`` with one ``MetadataClaim`` per recognized
    field.

    Reuses the repo source_id resolution path (``extract.repo``) so
    metadata claims share the same provenance pinning as
    empirical-extraction repo claims.
    """
    from . import repo as repo_walker

    repo_path = repo_path.resolve()
    if source_id is None:
        source_id, source_sha = repo_walker.resolve_source_id(repo_path)
    else:
        _, source_sha = repo_walker.resolve_source_id(repo_path)
    # Codex F-PR5b-CR3: prefer source-id-derived prefix to avoid
    # claim id collisions across repos with the same basename.
    fallback_slug = _slug(repo_path.name)
    repo_slug = _slug_prefix_from_source_id(source_id, fallback_slug)

    claims: list[MetadataClaim] = []
    skipped_files: list[SkippedFile] = []
    notes: list[str] = []

    # Each candidate file → its extractor function. The first slot
    # is the path RELATIVE to the walk root; for workspace members
    # we substitute the member's relative path so the source_file
    # field of the emitted claim points at the right config file.
    candidates = [
        ("pyproject.toml", "pyproject.toml", _extract_pyproject),
        ("Cargo.toml", "Cargo.toml", _extract_cargo_toml),
        ("package.json", "package.json", _extract_package_json),
    ]
    # Workspace expansion: if a root config is a pure workspace
    # declaration (no [package] / no [project]) it would yield zero
    # claims today. Detect the workspace.members / tool.uv.workspace
    # members list and walk each one as a discovered config file
    # alongside the root config. The member's slug is derived from
    # its directory basename so claim ids stay unique across
    # workspace members.
    workspace_members = _detect_workspace_members(repo_path)
    workspace_candidates: list[tuple[str, str, callable]] = []
    for member_rel in workspace_members:
        member_dir = repo_path / member_rel
        if not member_dir.is_dir():
            continue
        for rel_name, _, fn in candidates:
            member_config = member_dir / rel_name
            if member_config.is_file():
                # display_name now includes the member dir so
                # EXTRACTION.md surfaces "imspy_connector/Cargo.toml"
                # rather than "Cargo.toml" twice.
                workspace_candidates.append(
                    (
                        f"{member_rel}/{rel_name}",
                        f"{member_rel}/{rel_name}",
                        fn,
                    )
                )

    for rel_name, display_name, fn in candidates + workspace_candidates:
        path = repo_path / rel_name
        if not path.is_file():
            continue
        # Workspace members use a member-derived slug so per-member
        # claim ids don't collide across the workspace.
        if "/" in rel_name:
            member_dir = rel_name.rsplit("/", 1)[0]
            member_basename = member_dir.rsplit("/", 1)[-1]
            file_slug = f"{repo_slug}-{_slug(member_basename)}"
        else:
            file_slug = repo_slug
        # source_file_label is the repo-relative path so the emitted
        # claim's source_file pinpoints the actual config file in the
        # workspace tree.
        result = fn(path, file_slug, source_file_label=rel_name)
        # Codex F-PR5b-CR2: parse-error returns are a tuple
        # (None, detail). list returns are claims (possibly empty).
        if isinstance(result, tuple) and result[0] is None:
            detail = result[1]
            skipped_files.append(
                SkippedFile(
                    path=rel_name,
                    reason="parse_error",
                    detail=detail,
                )
            )
            notes.append(
                f"{display_name}: parse failed — {detail}"
            )
            continue
        new_claims = result if isinstance(result, list) else []
        if not new_claims:
            skipped_files.append(
                SkippedFile(
                    path=rel_name,
                    reason="no_recognised_fields",
                )
            )
            notes.append(
                f"{display_name}: parsed but no recognised "
                "compatibility fields"
            )
        else:
            claims.extend(new_claims)

    if not claims and not skipped_files:
        if workspace_members:
            notes.append(
                "workspace declared but no recognized config files "
                "found in members"
            )
        else:
            notes.append(
                "no pyproject.toml / Cargo.toml / package.json found "
                "at repo root or in workspace members"
            )

    return MetadataWalkResult(
        source_id=source_id,
        source_sha=source_sha,
        claims=claims,
        skipped_files=skipped_files,
        notes=notes,
    )


# ---------------------------------------------------------------------
# Render a walk result into the manifest dict shape
# ---------------------------------------------------------------------


def render_metadata_manifest(
    result: MetadataWalkResult,
    *,
    project: str,
    extractor_label: str = "evident-agent.extract-metadata",
    extracted_at: Optional[str] = None,
) -> dict:
    """Build the manifest dict for a metadata extraction.

    Output shape: one ``kind: metadata_compatibility`` claim per
    declared compatibility field. tier: research (curator decides
    if any merit higher tiers via PromoteFromExtracted later).
    No ``tolerances`` or ``evidence.command`` — the declaration IS
    the evidence.
    """
    from datetime import datetime, timezone

    if extracted_at is None:
        extracted_at = (
            datetime.now(tz=timezone.utc)
            .replace(microsecond=0)
            .isoformat()
            .replace("+00:00", "Z")
        )

    claim_blocks: list[dict] = []
    for c in result.claims:
        claim_blocks.append(
            {
                "id": c.id,
                "title": c.title,
                "kind": "metadata_compatibility",
                "tier": "research",
                "source": c.source_file,
                "case": f"{c.source_file}#{c.source_path}",
                "claim": c.claim,
                "metadata": {
                    "field": c.metadata_field,
                    "declared_value": c.declared_value,
                    "source_file": c.source_file,
                    "source_path": c.source_path,
                },
                "provenance": {
                    "kind": "extracted-from-repo",
                    "source_id": result.source_id,
                    "source_sha": result.source_sha,
                    "source_context": "repo_authored",
                    "extractor": {
                        "model": extractor_label,
                        "extracted_at": extracted_at,
                    },
                    "curator": None,
                },
            }
        )

    return {
        "version": "0.1",
        "project": project,
        "claims": claim_blocks,
    }


def run_extract_metadata(
    repo_path: Path,
    output_dir: Path,
    project: Optional[str] = None,
) -> MetadataWalkResult:
    """Walk a repo's metadata, render the draft manifest, and write
    ``evident.yaml`` + ``EXTRACTION.md`` into ``output_dir``.

    Shared by the ``extract-metadata`` CLI command and the
    ``evident-agent-mcp`` ``extract_metadata`` tool so there is a single
    writer of the output directory. Returns the walk result.
    """
    import yaml as _yaml

    result = walk_repo_metadata(repo_path)
    if project is None:
        project = f"extracted/{repo_path.resolve().name}-metadata"
    manifest = render_metadata_manifest(result, project=project)

    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "evident.yaml").write_text(
        _yaml.safe_dump(manifest, sort_keys=False, default_flow_style=False),
        encoding="utf-8",
    )

    # Brief EXTRACTION.md so the operator can see what was emitted.
    lines = [
        "# Metadata extraction summary\n",
        f"Source: `{result.source_id}` (sha256: `{result.source_sha}`)\n",
        f"\n## Emitted claims ({len(result.claims)})\n",
    ]
    for c in result.claims:
        lines.append(
            f"- **{c.id}** — {c.title}  \n"
            f"  `{c.source_file}::{c.source_path}` = `{c.declared_value}`"
        )
    if result.skipped_files:
        lines.append(f"\n## Skipped files ({len(result.skipped_files)})\n")
        for s in result.skipped_files:
            if s.reason == "parse_error":
                detail = s.detail or "parse error"
                lines.append(f"- `{s.path}` — **parse error**: {detail}")
            else:
                lines.append(f"- `{s.path}` (parsed but no recognised fields)")
    if result.notes:
        lines.append("\n## Notes\n")
        for n in result.notes:
            lines.append(f"- {n}")
    (output_dir / "EXTRACTION.md").write_text(
        "\n".join(lines) + "\n", encoding="utf-8",
    )

    return result
