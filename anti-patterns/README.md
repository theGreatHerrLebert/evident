# Anti-Patterns

Common ways systems appear correct while being wrong.

---

## Test-Passing Illusion
All tests pass, but tests encode incorrect assumptions.

---

## Agreement Trap
Multiple tools agree because they share the same flaw.

---

## Opaque Acceleration
High-performance code (GPU/AI) is used without understanding or validation.

---

## Benchmark Overfitting
System performs well on known datasets but fails in practice.

---

## Plausible Output Bias
Results look reasonable and are accepted without verification.

---

## Validation Theater
Many benchmarks, plots, or comparisons are produced, but they do not state the
trust question, oracle, tolerance, assumption, or decision rule they support.

Why it is dangerous:
- evidence volume is mistaken for evidence quality
- teams optimize visible metrics without knowing what they prove
- failures are hard to interpret because no claim was pinned in advance

Recovery:
- name the claim being validated
- identify the oracle or reference
- declare the tolerance before running the comparison
- document what result would block release or require redesign
