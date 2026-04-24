# Proteon: Oracle-Backed Release Readiness

Source repo: [`cases/proteon`](proteon/)

## Problem

Proteon is a Rust-first structural bioinformatics toolkit with Python and CLI
entry points. It implements scientific compute components where plausible
outputs are not enough: structure I/O, alignment, SASA, DSSP, hydrogen
placement, force-field energies, minimization, search, and dataset export.

The trust problem is release-readiness: when a project is close to being used
as a scientific compute kernel, how much evidence is enough to justify shipping
numerical and structural-biology claims?

## Trust Strategy

Proteon relies primarily on validation, supported by implementation
understanding and explicit attribution.

- Understanding: algorithms are decomposed into focused Rust crates and Python
  boundaries, with public surfaces documented in the README.
- Validation: numerical claims are checked against external tools such as
  OpenMM, BALL, USAlign, Biopython, Gemmi, FreeSASA, pydssp, reduce, MMseqs2,
  and Foldseek.
- Proof: not the main strategy. The project uses tolerance-bounded empirical
  evidence rather than formal guarantees.

## Evidence

The strongest evidence is that Proteon treats oracle validation as a release
discipline, not an afterthought.

- `docs/ORACLE_SETUP.md` gives pinned oracle versions and reproducibility
  commands.
- `tests/oracle/README.md` lists current oracles, what each covers, and how
  tests are expected to fail.
- `devdocs/ORACLE.md` explains the tolerance philosophy and the difference
  between oracle tests and frozen fixtures.
- `THIRD_PARTY_NOTICES.md` separates incorporated source, paper-inspired
  implementations, linked dependencies, and development-time correctness
  oracles.
- CI exists and runs a practical subset of the validation story.

## Assumptions

- External tools are treated as authoritative until disagreement is explained.
- Tolerances are part of the scientific claim, not implementation noise.
- Some disagreements are convention gaps rather than bugs, but those gaps must
  be documented.
- Heavy oracles may be skipped in fast CI, provided release-quality validation
  remains reproducible.

## Failure Modes

- Agreement with one oracle can hide a shared convention or modeling error.
- Golden fixtures can drift if they are not tied back to live oracle
  regeneration.
- Release claims can outgrow CI if heavy oracle checks are not run before tags.
- Licensing risk appears when reimplementing or porting algorithms from
  established tools without preserving provenance and license boundaries.

## What Worked

Proteon makes trust operational. A numerical claim is expected to have an
oracle, a tolerance, an installation recipe, and diagnostic failure output. The
project also distinguishes different kinds of external influence: copied or
ported code, linked dependencies, paper-level inspiration, and tools used only
for validation.

This is a strong example of the rule "testing is not enough." Unit tests exist,
but the higher-order trust comes from independent implementations and explicit
reproduction paths.

## What Is Still Lacking

The validation system is strong, but it is expensive and partly dependent on
tools that are hard to install. That creates a split between everyday CI and
release-grade validation. The project needs clear release gates so users know
which evidence was actually rerun for a version.

Search is also explicitly pre-product. That honesty is good, but it means the
trust envelope is not uniform across the repo: core structure compute is much
closer to release quality than experimental retrieval.

## Evident Lessons

- Release readiness should be tied to oracle-backed claims, not only test
  coverage.
- Tolerances must be documented as part of the claim.
- Licensing and attribution are part of trust, not administrative cleanup.
- A mature scientific compute repo should distinguish fast CI evidence from
  heavier release evidence.
