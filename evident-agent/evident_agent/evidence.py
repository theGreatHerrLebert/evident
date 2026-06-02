"""Per-format evidence-digest extractors for ``evident-agent review``.

The model that authors a ReviewEvent never sees the raw artifact.
Instead the agent extracts a *digest*: a structured selection of the
parts of the artifact that bear on the claim's tolerance, plus a header
that records what was extracted and whether anything was truncated.

Each extractor returns a ``Digest`` (header + body + ``truncated``
flag). The dispatcher (`make_digest`) chooses one by file extension.

Why not just send the full artifact: tokens are expensive and large
artifacts can drown out the line that matters. Why not summarize: that
would smuggle semantic judgment into the agent. Extractors only select
and structure; they do not interpret.

Constants:
    MAX_BODY_BYTES — soft cap on body size. JSON-like extractors aim
        well under it; the fallback truncates here.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

MAX_BODY_BYTES = 4096
PYTEST_TAIL_LINES = 80


@dataclass
class Digest:
    """One artifact rendered into prompt-ready form.

    ``header`` carries format, source path, optional commit, and
    ``metric_present`` (``"pass" | "fail" | "unknown"``) — the same
    vocabulary the model uses in its submit_review checks block.
    ``body`` is the structured content the model will read.
    ``truncated`` is true whenever the extractor dropped content that
    could have been relevant to verifying the metric.
    """

    header: dict[str, Any] = field(default_factory=dict)
    body: str = ""
    truncated: bool = False

    def render(self) -> str:
        """Render the digest as the text block the model sees."""
        header_line = json.dumps(self.header, sort_keys=True)
        return f"<digest header=\"{header_line}\">\n{self.body}\n</digest>"


def make_digest(
    artifact_path: Path,
    metric: Optional[str],
    *,
    source_dir: Optional[Path] = None,
    commit: Optional[str] = None,
) -> Digest:
    """Pick an extractor by file extension and return its Digest.

    ``metric`` is the claim's tolerance metric (dotted-path or simple
    name); extractors use it to bias what they include. When None, the
    extractor still runs but cannot mark ``metric_present`` confidently.
    """
    suffix = artifact_path.suffix.lower()
    if suffix in (".json", ".jsonl"):
        return extract_json(artifact_path, metric, source_dir=source_dir, commit=commit)
    if suffix in (".csv", ".tsv"):
        return extract_csv(artifact_path, metric, source_dir=source_dir, commit=commit)
    if suffix in (".log", ".txt"):
        return extract_pytest_or_log(artifact_path, metric, source_dir=source_dir, commit=commit)
    return extract_fallback(artifact_path, metric, source_dir=source_dir, commit=commit)


# ---------- JSON / JSONL ----------

def extract_json(
    path: Path,
    metric: Optional[str],
    *,
    source_dir: Optional[Path] = None,
    commit: Optional[str] = None,
) -> Digest:
    """Extract from JSON / JSONL.

    Strategy: parse fully; pull out the field matching ``metric`` and
    its immediate context; include the top-level summary block.
    Truncate to MAX_BODY_BYTES with a marker.
    """
    text = _safe_read_text(path)
    if text is None:
        return _missing_digest(path, "json", source_dir, commit)

    body_obj: Any
    try:
        if path.suffix.lower() == ".jsonl":
            records = [json.loads(line) for line in text.splitlines() if line.strip()]
            body_obj = _summarize_records(records, metric)
        else:
            parsed = json.loads(text)
            body_obj = _summarize_json(parsed, metric)
    except json.JSONDecodeError as exc:
        return Digest(
            header={
                "format": "json",
                "source": _format_source(path, source_dir),
                "commit": commit,
                "metric_present": "unknown",
                "parse_error": str(exc),
            },
            body=text[:MAX_BODY_BYTES],
            truncated=len(text.encode("utf-8")) > MAX_BODY_BYTES,
        )

    pretty = json.dumps(body_obj, indent=2, sort_keys=True)
    truncated = False
    if len(pretty.encode("utf-8")) > MAX_BODY_BYTES:
        pretty = pretty[:MAX_BODY_BYTES] + "\n...<truncated>...\n"
        truncated = True

    metric_present = _metric_present(body_obj, metric)
    return Digest(
        header={
            "format": "json" if path.suffix.lower() == ".json" else "jsonl",
            "source": _format_source(path, source_dir),
            "commit": commit,
            "metric_present": metric_present,
        },
        body=pretty,
        truncated=truncated,
    )


def _summarize_json(obj: Any, metric: Optional[str]) -> Any:
    """Build a minimal projection of a parsed JSON value.

    For a dict: include the top-level summary keys plus the dotted-path
    descent for ``metric`` (when present). For an array of records:
    delegate to ``_summarize_records``.
    """
    if isinstance(obj, list):
        return _summarize_records(obj, metric)

    if not isinstance(obj, dict):
        # Scalar — already minimal.
        return obj

    out: dict[str, Any] = {}

    # Surface common headline keys so the model has anchor context.
    for k in ("summary", "metric", "value", "observed", "primary_metric"):
        if k in obj:
            out[k] = obj[k]

    if metric:
        path_value = _dig(obj, metric)
        if path_value is not None:
            out[metric] = path_value

    # If the projection ends up empty (nothing matched), keep the
    # original top-level keys so the model isn't working blind.
    if not out:
        return obj
    return out


def _summarize_records(records: list[Any], metric: Optional[str]) -> dict[str, Any]:
    """Build a summary block for a list of records (JSONL or a JSON
    array). Includes count + min/max/mean/median of the ``metric`` field
    when numeric, plus up to 5 sample rows.
    """
    out: dict[str, Any] = {"records": len(records)}
    if not records:
        return out

    if metric:
        values: list[float] = []
        for r in records:
            v = _dig(r, metric) if isinstance(r, dict) else None
            if isinstance(v, (int, float)):
                values.append(float(v))
        if values:
            values_sorted = sorted(values)
            out[f"{metric}_summary"] = {
                "count": len(values),
                "min": values_sorted[0],
                "max": values_sorted[-1],
                "median": values_sorted[len(values_sorted) // 2],
                "mean": sum(values) / len(values),
            }

    out["sample_rows"] = records[:5]
    return out


# ---------- CSV / TSV ----------

def extract_csv(
    path: Path,
    metric: Optional[str],
    *,
    source_dir: Optional[Path] = None,
    commit: Optional[str] = None,
) -> Digest:
    """Extract from CSV / TSV: header, row count, min/max/median of the
    metric column (if found), plus a few sample rows."""
    text = _safe_read_text(path)
    if text is None:
        return _missing_digest(path, "csv", source_dir, commit)

    sep = "\t" if path.suffix.lower() == ".tsv" else ","
    lines = text.splitlines()
    if not lines:
        return Digest(
            header={
                "format": "csv",
                "source": _format_source(path, source_dir),
                "commit": commit,
                "metric_present": "unknown",
            },
            body="(empty file)",
            truncated=False,
        )

    header = [h.strip() for h in lines[0].split(sep)]
    rows = [line.split(sep) for line in lines[1:] if line.strip()]

    metric_present: str = "unknown"
    metric_summary: Optional[dict[str, float]] = None
    if metric:
        if metric in header:
            metric_present = "pass"
            idx = header.index(metric)
            values: list[float] = []
            for r in rows:
                if idx < len(r):
                    try:
                        values.append(float(r[idx]))
                    except ValueError:
                        pass
            if values:
                vs = sorted(values)
                metric_summary = {
                    "count": len(values),
                    "min": vs[0],
                    "max": vs[-1],
                    "median": vs[len(vs) // 2],
                    "mean": sum(values) / len(values),
                }
        else:
            metric_present = "fail"

    sample = "\n".join(lines[: min(10, len(lines))])
    body_lines = [
        f"columns: {header}",
        f"rows: {len(rows)}",
    ]
    if metric_summary is not None:
        body_lines.append(f"{metric} summary: {json.dumps(metric_summary)}")
    body_lines.append("first rows:")
    body_lines.append(sample)
    body = "\n".join(body_lines)
    truncated = len(body.encode("utf-8")) > MAX_BODY_BYTES
    if truncated:
        body = body[:MAX_BODY_BYTES] + "\n...<truncated>...\n"

    return Digest(
        header={
            "format": "csv",
            "source": _format_source(path, source_dir),
            "commit": commit,
            "metric_present": metric_present,
        },
        body=body,
        truncated=truncated,
    )


# ---------- pytest / log output ----------

def extract_pytest_or_log(
    path: Path,
    metric: Optional[str],
    *,
    source_dir: Optional[Path] = None,
    commit: Optional[str] = None,
) -> Digest:
    """Extract from pytest / log output.

    Strategy: last N lines (final summary block), plus any earlier
    lines matching the metric name or failure keywords. Deduplicated,
    capped at MAX_BODY_BYTES.
    """
    text = _safe_read_text(path)
    if text is None:
        return _missing_digest(path, "log", source_dir, commit)

    lines = text.splitlines()
    keep: list[str] = []
    seen: set[int] = set()

    keywords_re = re.compile(r"(FAILED|ERROR|error:|outlier|warning|WARNING)", re.IGNORECASE)
    metric_re = re.compile(re.escape(metric), re.IGNORECASE) if metric else None

    for i, line in enumerate(lines):
        if keywords_re.search(line):
            if i not in seen:
                keep.append(line)
                seen.add(i)
        elif metric_re is not None and metric_re.search(line):
            if i not in seen:
                keep.append(line)
                seen.add(i)

    tail_start = max(0, len(lines) - PYTEST_TAIL_LINES)
    for i in range(tail_start, len(lines)):
        if i not in seen:
            keep.append(lines[i])
            seen.add(i)

    body = "\n".join(keep)
    truncated = False
    if len(body.encode("utf-8")) > MAX_BODY_BYTES:
        body = body[-MAX_BODY_BYTES:]  # keep the tail
        truncated = True

    metric_present = (
        "pass" if metric and metric_re is not None and metric_re.search(body) else
        ("unknown" if metric is None else "fail")
    )
    return Digest(
        header={
            "format": "log",
            "source": _format_source(path, source_dir),
            "commit": commit,
            "metric_present": metric_present,
        },
        body=body,
        truncated=truncated,
    )


# ---------- fallback ----------

def extract_fallback(
    path: Path,
    metric: Optional[str],
    *,
    source_dir: Optional[Path] = None,
    commit: Optional[str] = None,
) -> Digest:
    """Last-resort extractor: tail MAX_BODY_BYTES of the file as text,
    flagged ``format_unknown: true`` so the prompt rules force a
    Dissent when the cited metric value can't be found in the tail.
    """
    text = _safe_read_text(path)
    if text is None:
        return _missing_digest(path, "unknown", source_dir, commit)

    encoded = text.encode("utf-8", errors="replace")
    truncated = len(encoded) > MAX_BODY_BYTES
    body_bytes = encoded[-MAX_BODY_BYTES:] if truncated else encoded
    body = body_bytes.decode("utf-8", errors="replace")

    metric_present = "unknown"
    if metric and re.search(re.escape(metric), body, flags=re.IGNORECASE):
        metric_present = "pass"
    elif metric:
        metric_present = "fail"

    return Digest(
        header={
            "format": "unknown",
            "format_unknown": True,
            "source": _format_source(path, source_dir),
            "commit": commit,
            "metric_present": metric_present,
        },
        body=body,
        truncated=truncated,
    )


# ---------- helpers ----------

def _safe_read_text(path: Path) -> Optional[str]:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except (OSError, FileNotFoundError):
        return None


def _missing_digest(
    path: Path,
    fmt: str,
    source_dir: Optional[Path],
    commit: Optional[str],
) -> Digest:
    return Digest(
        header={
            "format": fmt,
            "source": _format_source(path, source_dir),
            "commit": commit,
            "metric_present": "unknown",
            "missing": True,
        },
        body=f"(artifact at {path} could not be read)",
        truncated=False,
    )


def _format_source(path: Path, source_dir: Optional[Path]) -> str:
    """Render a stable, relative-to-source-dir path so digests are
    reproducible across machines."""
    try:
        if source_dir is not None:
            return str(path.relative_to(source_dir))
    except ValueError:
        pass
    return str(path)


def _dig(obj: Any, dotted: str) -> Any:
    """Descend ``obj`` along a dotted path (``a.b.c``). Returns None on
    miss. Mirrors proteon's claim_scoring._dig behavior."""
    cur: Any = obj
    for part in dotted.split("."):
        if isinstance(cur, dict) and part in cur:
            cur = cur[part]
        else:
            return None
    return cur


def _metric_present(obj: Any, metric: Optional[str]) -> str:
    if metric is None:
        return "unknown"
    if isinstance(obj, dict):
        if _dig(obj, metric) is not None:
            return "pass"
        # The summarizer may have surfaced the metric under a different
        # key; do a string search as a fallback before concluding "fail".
        if metric in json.dumps(obj):
            return "pass"
        return "fail"
    return "unknown"
