"""Allow-list path policy — a hardened Python port of typed-trust's
``AllowListPathPolicy`` (loader.rs:228-314).

Roots are registered with ``--allow-root`` and canonicalized at
registration. Every tool-call path is canonicalized again and must
either equal an allowed file root exactly, or live under an allowed
directory root.

**Hardening beyond the Rust original** (Codex review, High): the Rust
server only ever reads existing manifests, so it can always
``canonicalize`` strictly. This server also *writes* (replay sidecar,
extract ``output_dir``) and *bind-mounts into docker*, so it must
authorize not-yet-created paths. The ``allow_missing`` mode does that
without opening a symlink/TOCTOU hole:

1. Existing-path checks resolve symlinks (``realpath``) before matching,
   so a symlink whose target escapes an allowed root is rejected.
2. ``allow_missing`` authorizes the **nearest existing ancestor**
   (symlink-resolved) and appends only the literal, not-yet-existing
   trailing components — which cannot themselves contain symlinks. A
   ``..`` in that trailing remainder is rejected outright.
3. :meth:`recheck` re-validates a path *after* it has been created
   (strict realpath), so a symlink swapped in between authorization and
   write is caught before the caller trusts the location.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import List


class PolicyDenied(Exception):
    """A path failed the allow-list check (or the policy is unconfigured)."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


class AllowListPathPolicy:
    def __init__(self) -> None:
        self._files: List[Path] = []  # canonical file roots (exact match)
        self._dirs: List[Path] = []  # canonical dir roots (subtree)

    # ------------------------------------------------------------------
    # Registration
    # ------------------------------------------------------------------
    def allow(self, path: os.PathLike | str) -> None:
        """Register an allowed path. Canonicalizes immediately; a
        non-existent/inaccessible path raises so the server fails at
        startup rather than pretending to be configured."""
        p = Path(path)
        try:
            canonical = p.resolve(strict=True)
        except OSError as exc:
            raise PolicyDenied(
                f"cannot canonicalize allow-list path {p} ({exc})"
            )
        if canonical.is_dir():
            self._dirs.append(canonical)
        elif canonical.is_file():
            self._files.append(canonical)
        else:
            raise PolicyDenied(
                f"{canonical} is neither a file nor a directory; "
                "allow-list entries must be one"
            )

    def is_empty(self) -> bool:
        return not self._files and not self._dirs

    # ------------------------------------------------------------------
    # Checking
    # ------------------------------------------------------------------
    def check(self, path: os.PathLike | str, *, allow_missing: bool = False) -> Path:
        """Return the canonical path if allowed, else raise
        :class:`PolicyDenied`.

        With ``allow_missing=False`` the path must already exist (used
        for manifests, repos, papers). With ``allow_missing=True`` the
        path may not exist yet (used for the replay sidecar and the
        extract ``output_dir``); authorization is anchored on its
        nearest existing ancestor.
        """
        if self.is_empty():
            raise PolicyDenied("no allowed roots configured (use --allow-root)")

        if not allow_missing:
            try:
                canonical = Path(path).resolve(strict=True)
            except OSError as exc:
                raise PolicyDenied(f"path not accessible ({path}): {exc}")
            return self._match(canonical)

        ancestor, trailing = self._existing_ancestor(Path(path))
        if any(part == ".." for part in trailing):
            raise PolicyDenied(
                f"path {path} contains '..' beyond an existing component"
            )
        canonical = ancestor.joinpath(*trailing) if trailing else ancestor
        # The existing, symlink-resolved ancestor must be inside a root;
        # the literal trailing parts only extend it deeper.
        self._match(ancestor)
        return self._match(canonical)

    def recheck(self, path: os.PathLike | str) -> Path:
        """Re-validate a path that has since been created. Strict
        realpath catches a symlink swapped in after a prior
        ``allow_missing`` authorization."""
        return self.check(path, allow_missing=False)

    # ------------------------------------------------------------------
    # Internals
    # ------------------------------------------------------------------
    def _match(self, canonical: Path) -> Path:
        for f in self._files:
            if canonical == f:
                return canonical
        for d in self._dirs:
            if canonical == d or canonical.is_relative_to(d):
                return canonical
        raise PolicyDenied(f"{canonical} not under any allowed root")

    @staticmethod
    def _existing_ancestor(p: Path) -> tuple[Path, List[str]]:
        """Return (realpath of nearest existing ancestor, literal trailing
        components that do not yet exist)."""
        p = Path(os.path.abspath(p))
        trailing: List[str] = []
        cur = p
        while not cur.exists():
            trailing.append(cur.name)
            parent = cur.parent
            if parent == cur:  # reached filesystem root
                break
            cur = parent
        real_ancestor = Path(os.path.realpath(cur))
        trailing.reverse()
        return real_ancestor, trailing
