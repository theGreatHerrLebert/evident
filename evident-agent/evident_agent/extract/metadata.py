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
    path: Path, repo_slug: str,
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
                source_file="pyproject.toml",
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
                source_file="pyproject.toml",
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
                source_file="pyproject.toml",
                source_path="project.version",
            )
        )
    return out


def _extract_cargo_toml(
    path: Path, repo_slug: str,
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
                source_file="Cargo.toml",
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
                source_file="Cargo.toml",
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
                source_file="Cargo.toml",
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
                source_file="Cargo.toml",
                source_path="package.version",
            )
        )
    return out


def _extract_package_json(
    path: Path, repo_slug: str,
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
                source_file="package.json",
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
                source_file="package.json",
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
                    source_file="package.json",
                    source_path="engines.node",
                )
            )
    return out


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

    # Each candidate file → its extractor function.
    candidates = [
        ("pyproject.toml", "pyproject.toml", _extract_pyproject),
        ("Cargo.toml", "Cargo.toml", _extract_cargo_toml),
        ("package.json", "package.json", _extract_package_json),
    ]
    for rel_name, display_name, fn in candidates:
        path = repo_path / rel_name
        if not path.is_file():
            continue
        result = fn(path, repo_slug)
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
        notes.append(
            "no pyproject.toml / Cargo.toml / package.json found "
            "at repo root"
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
