# Typed Trust rollup

**Manifest:** `experiments/rustims-concordance-demo/evident.yaml`  
**Reports:** 10 (10 current, 0 contested, 0 superseded)  
**Skipped:** 0 (out of scope or translation error)  

---

# Trust Report

**Claim:** `rustims-maxquant-peak-matching-7p5min-150k`  
**Status:** Current ✓

## Concordance result

- **Status:** Not assessed
- **Diagnostics:**
    - `metric_path`: `"maxquant.peak_matching_error.fraction_pct_7p5min_150k"`
    - `reason`: `"path component 'fraction_pct_7p5min_150k' not present in artifact"`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`

## Observation

- **Pattern:** `numeric_band`
- **Third-party tool:** `MaxQuant`
- **Metric definition:** `MaxQuant peak matching error rate per Cox 2008 §Methods — fraction of MS1 alignment peaks MaxQuant failed to bind to a precursor, expressed as a percentage of total peaks examined. `
- **Paper locator:** `page-6, lines 100-101`
- **Metric path:** `maxquant.peak_matching_error.fraction_pct_7p5min_150k`
- **Observed value:** `30.0`
- **Epsilon:** `5.0`

## Observation result

- **Status:** Not assessed
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`


---

# Trust Report

**Claim:** `rustims-peaks-xpro-fdr-hla-10k`  
**Status:** Current ✓

## Concordance result

- **Status:** Pass ✓
- **Observed value:** `1.75`
- **Diagnostics:**
    - `delta_from_prior`: `-0.05000000000000005`
    - `epsilon`: `0.5`
    - `within_band`: `true`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`

## Observation

- **Pattern:** `numeric_band`
- **Third-party tool:** `PEAKS-XPro`
- **Metric definition:** `Empirical true FDR (rustims §Methods): fraction of peptide identifications labeled positive by PEAKS-XPro at q ≤ 0.01 that were not in the gold-standard set. `
- **Paper locator:** `page-9, line 237`
- **Metric path:** `peaks_xpro.hla_10k.real_fdr_pct`
- **Observed value:** `1.8`
- **Epsilon:** `0.5`

## Observation result

- **Status:** Pass ✓
- **Observed value:** `1.75`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`


---

# Trust Report

**Claim:** `rustims-peaks-xpro-fdr-hla-100k`  
**Status:** Current ✓

## Concordance result

- **Status:** Pass ✓
- **Observed value:** `1.2`
- **Diagnostics:**
    - `delta_from_prior`: `0.05000000000000005`
    - `epsilon`: `0.5`
    - `within_band`: `true`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`

## Observation

- **Pattern:** `numeric_band`
- **Third-party tool:** `PEAKS-XPro`
- **Metric definition:** `Empirical true FDR on the 100k HLA-I dataset, same definition as the 10k claim above. `
- **Paper locator:** `page-9, line 237`
- **Metric path:** `peaks_xpro.hla_100k.real_fdr_pct`
- **Observed value:** `1.15`
- **Epsilon:** `0.5`

## Observation result

- **Status:** Pass ✓
- **Observed value:** `1.2`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`


---

# Trust Report

**Claim:** `rustims-fragpipe-fdr-hla-10k`  
**Status:** Current ✓

## Concordance result

- **Status:** Pass ✓
- **Observed value:** `0.95` `percentage_points`
- **Diagnostics:**
    - `delta_from_prior`: `0.03999999999999993`
    - `epsilon`: `0.3`
    - `within_band`: `true`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`

## Observation

- **Pattern:** `numeric_band`
- **Third-party tool:** `FragPipe`
- **Metric definition:** `Empirical true FDR — same definition as the PEAKS-XPro 10k claim above. `
- **Paper locator:** `page-9, line 237`
- **Metric path:** `fragpipe.hla_10k.real_fdr_pct`
- **Observed value:** `0.91`
- **Epsilon:** `0.3`

## Observation result

- **Status:** Pass ✓
- **Observed value:** `0.95` `percentage_points`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`


---

# Trust Report

**Claim:** `rustims-fragpipe-fdr-hla-100k`  
**Status:** Current ✓

## Concordance result

- **Status:** Fail ✗
- **Observed value:** `1.55`
- **Diagnostics:**
    - `delta_from_prior`: `0.3900000000000001`
    - `epsilon`: `0.3`
    - `within_band`: `false`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`

## Observation

- **Pattern:** `numeric_band`
- **Third-party tool:** `FragPipe`
- **Metric definition:** `Empirical true FDR — same definition as the 10k claim above. `
- **Paper locator:** `page-9, line 237`
- **Metric path:** `fragpipe.hla_100k.real_fdr_pct`
- **Observed value:** `1.16`
- **Epsilon:** `0.3`

## Observation result

- **Status:** Fail ✗
- **Observed value:** `1.55`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`


---

# Trust Report

**Claim:** `rustims-fragpipe-identification-hla-10k`  
**Status:** Current ✓

## Concordance result

- **Status:** Pass ✓
- **Observed value:** `36.4` `percentage_points`
- **Diagnostics:**
    - `delta_from_prior`: `-0.3999999999999986`
    - `epsilon`: `2.0`
    - `within_band`: `true`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`

## Observation

- **Pattern:** `numeric_band`
- **Third-party tool:** `FragPipe`
- **Metric definition:** `Fraction of MS2-fragmented peptides FragPipe was able to identify, expressed as a percentage of the total fragmented set (9,825 for the 10k dataset). `
- **Paper locator:** `page-9, lines 238-240`
- **Metric path:** `fragpipe.hla_10k.identification_rate_pct`
- **Observed value:** `36.8`
- **Epsilon:** `2.0`

## Observation result

- **Status:** Pass ✓
- **Observed value:** `36.4` `percentage_points`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`


---

# Trust Report

**Claim:** `rustims-peaks-xpro-identification-hla-10k`  
**Status:** Current ✓

## Concordance result

- **Status:** Pass ✓
- **Observed value:** `32.7`
- **Diagnostics:**
    - `delta_from_prior`: `-0.19999999999999576`
    - `epsilon`: `2.0`
    - `within_band`: `true`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`

## Observation

- **Pattern:** `numeric_band`
- **Third-party tool:** `PEAKS-XPro`
- **Metric definition:** `Fraction of MS2-fragmented peptides PEAKS-XPro was able to identify, same denominator as the FragPipe claim above. `
- **Paper locator:** `page-9, lines 238-240`
- **Metric path:** `peaks_xpro.hla_10k.identification_rate_pct`
- **Observed value:** `32.9`
- **Epsilon:** `2.0`

## Observation result

- **Status:** Pass ✓
- **Observed value:** `32.7`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`


---

# Trust Report

**Claim:** `rustims-fragpipe-identification-hla-100k`  
**Status:** Current ✓

## Concordance result

- **Status:** Pass ✓
- **Observed value:** `30.7`
- **Diagnostics:**
    - `delta_from_prior`: `0.1999999999999993`
    - `epsilon`: `2.0`
    - `within_band`: `true`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`

## Observation

- **Pattern:** `numeric_band`
- **Third-party tool:** `FragPipe`
- **Metric definition:** `Same as the 10k claim, scaled to the 100k denominator (92,047 fragmented peptides). `
- **Paper locator:** `page-9, lines 238-240`
- **Metric path:** `fragpipe.hla_100k.identification_rate_pct`
- **Observed value:** `30.5`
- **Epsilon:** `2.0`

## Observation result

- **Status:** Pass ✓
- **Observed value:** `30.7`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`


---

# Trust Report

**Claim:** `rustims-peaks-xpro-identification-hla-100k`  
**Status:** Current ✓

## Concordance result

- **Status:** Fail ✗
- **Observed value:** `25`
- **Diagnostics:**
    - `delta_from_prior`: `-4.5`
    - `epsilon`: `2.0`
    - `within_band`: `false`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`

## Observation

- **Pattern:** `numeric_band`
- **Third-party tool:** `PEAKS-XPro`
- **Metric definition:** `Same as the FragPipe 100k claim. `
- **Paper locator:** `page-9, lines 238-240`
- **Metric path:** `peaks_xpro.hla_100k.identification_rate_pct`
- **Observed value:** `29.5`
- **Epsilon:** `2.0`

## Observation result

- **Status:** Fail ✗
- **Observed value:** `25`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`


---

# Trust Report

**Claim:** `rustims-fdr-tool-ordering-hla-10k`  
**Status:** Current ✓

## Concordance result

- **Status:** Pass ✓
- **Observed ordering:** `FragPipe` → `PEAKS_XPro`
- **Prior ordering:** `FragPipe` → `PEAKS_XPro`
- **Diagnostics:**
    - `direction`: `"lower_is_better"`
    - `measured_values`: `{"FragPipe":0.95,"PEAKS_XPro":1.75}`
    - `prior_values`: `{"FragPipe":0.91,"PEAKS_XPro":1.8}`
    - `tie_policy`: `"adjacent_swap_ok"`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`

## Observation

- **Pattern:** `ordinal_match`
- **Third-party tool:** `FragPipe, PEAKS-XPro`
- **Metric definition:** `Real FDR per the per-tool claims above; ordering is the load-bearing property when individual cell values shift slightly. `
- **Paper locator:** `page-9, line 237`
- **Direction:** `lower_is_better`
- **Per-entity observed values:**
    - `FragPipe`: `0.91`
    - `PEAKS_XPro`: `1.8`

## Observation result

- **Status:** Pass ✓
- **Observed ordering:** `FragPipe` → `PEAKS_XPro`
- **Image digest:** `sha256:demo-image-placeholder`
- **Produced at:** `2026-06-04T12:00:00Z`


---

