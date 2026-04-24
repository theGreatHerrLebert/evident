# Rules

Rules are actionable guidelines. They should be specific, checkable, and useful in practice.

---

## Core Rules

### 1. Declare Trust Strategy
Every component must state how its correctness is established:
- understanding
- validation
- proof

---

### 2. Compensate for Limited Understanding
If a component is not fully understood, stronger validation is required.

---

### 3. Treat Generated Code as Untrusted
AI-generated code must be assumed incorrect until validated.

---

### 4. Testing Is Not Enough
Passing tests does not guarantee correctness.
Validation must include independent evidence.

---

### 5. Use Independent Validation
Validation should rely on assumptions that differ from the implementation.

Agreement alone is not sufficient.

---

### 6. Document Failure Modes
Known limitations and edge cases must be explicitly stated.

---

### 7. Build from Trusted Components
Only validated components should be used as building blocks.

---

### 8. Ensure Reproducibility
Validation must be:
- versioned
- automated
- repeatable

---

### 9. Respect Attribution
Algorithms, tools, and influences must be acknowledged where known.

---

### 10. Check Legal Constraints
Generated or reimplemented code must comply with licensing requirements.
