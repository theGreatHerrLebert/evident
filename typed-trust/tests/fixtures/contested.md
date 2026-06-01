# Trust Report

**Claim:** `proteon-sasa-vs-biopython-release-1k-pdbs`  
**Status:** Contested ⚠

## Criteria

### median_relative_error < 0.005 on total_sasa vs Biopython

- **Result:** Pass ✓
- **Render status:** Contested ⚠
- **Observed value:** `0.0017`
- **Tolerance:** `median_relative_error < 0.005` on `total_sasa` vs `Biopython`
- **Contested by:**
  - `rev-jane-doe-tolerance-too-wide`

## Active Challenges

### `rev-jane-doe-tolerance-too-wide`

- **Kind:** challenge
- **Target:** criterion `proteon-sasa-vs-biopython-release-1k-pdbs-criterion-0`
- **By:** Jane Doe (human)
  - orcid: `0000-0000-0000-0001`
  - affiliation: `Example University`
- **Protocol:** `proteon-release-peer-review-v1`
- **Category:** weak_statistics
- **Backed by:** `synthetic-tolerance-too-wide`
- **Rationale:** Median 0.5% absorbs FreeSASA convention drift but leaves no room to detect proteon-side regressions of similar magnitude. Recommend tightening to 0.2%.

## Backing Claims

- `synthetic-tolerance-too-wide` — Current ✓ (1 criteria)

