# Patterns

Patterns are reusable solutions for establishing trust.

---

## Oracle Comparison

Compare outputs against a trusted external implementation.

---

## Simulation-Based Validation

Use synthetic data with known ground truth.

---

## Cross-Tool Agreement

Validate behavior across multiple independent tools.

---

## Reference Shadowing

Maintain a simple reference implementation alongside an optimized version.

---

## Property-Based Testing

Validate general properties rather than fixed outputs.

---

## Oracle-Backed Release Gate

Before release, every scientific or numerical claim must point to an
independent oracle, a documented tolerance, a reproducible command, and a
recorded result for the release commit.

Use when:
- the component is close to being used by others
- incorrect output could affect scientific conclusions
- external reference tools exist

Minimum evidence:
- oracle identity and version
- input dataset or fixture
- tolerance and rationale
- command to reproduce
- known convention gaps

Seen in:
- [Proteon case](../cases/proteon.md)

---

## Layered Evaluation Against Simulated and Real References

Evaluate a system at multiple layers, using simulated ground truth where exact
truth exists and real-world proxy tools where it does not.

Use when:
- the final task has no perfect oracle on real data
- simulated data is available but incomplete
- intermediate pipeline stages can fail independently

Minimum evidence:
- simulated-track metrics against known truth
- real-track metrics against reference tools or expert labels
- explicit statement of what each track can and cannot prove
- decision rules for promotion from experimental to validated

Seen in:
- [cu-ims-primitives case](../cases/cu-ims-primitives.md)
