> **Experimental PDF source.**
> PDF extraction is experimental. pdftotext-mangled column breaks can defeat the validator's local-binding check. Inspect the extracted text in dry-run mode before trusting a non-dry-run extraction.

# Extraction summary

Source: `preprint:rustims-supplement-df40ee33`  
Source sha256: `cf09b769e3804fbd10c8a6a1454696ca65fc8b37743a6a99d318b1a9d2f0d587`  
Extractor: `claude-opus-4-7` (2026-06-03T13:53:00Z)  


## Accepted claims (0)

_No claims extracted. Default-deny framing rejected all candidates._

## Rejected candidates (7)


### `bound_not_stated` (2)

- HeLa Samples (dda-PASEF), page 4: 'minimum precursor intensity of 1,000 and a dynamic exclusion window of 25 frames'  
  _Reason:_ These are simulation input parameters, not empirical bounds on performance.
- Table 3, page 14: 'minimum probability of a charge state, charge states below will be dropped (default 0.005)'  
  _Reason:_ Default parameter value, not an empirical performance claim about the system.

### `comparator_bound_to_wrong_subject` (4)

- Section 1.4, HLA1 Samples description, page 5: 'FDR ≤ 1%'  
  _Reason:_ The bound applies to a previously published external dataset of experimentally identified peptides used as input, not to a performance claim about timsim itself.
- Section 1.5.4 PEAKS, page 6: 'Results were filtered at FDR ≤ 0.01 for peptides and −10 log P ≥ 0 for proteins.'  
  _Reason:_ These are filtering settings applied within PEAKS configuration, not performance claims about timsim or any benchmarked subject.
- Section 1.5.2 FragPipe, page 5: 'ion level FDR set to 0.01 (default parameter)'  
  _Reason:_ This is a software configuration setting for FragPipe, not a performance claim.
- Section 1.5.4 PEAKS, page 6: 'Peptides were identified with mass accuracy thresholds of 15 ppm for MS1 and 0.03 Da for MS2.'  
  _Reason:_ These are PEAKS configuration thresholds, not performance claims about an artifact's behavior.

### `hedged_qualitative_only` (1)

- Section 2.1.3, page 13: 'mean central 76% interval width of a sample of curves is approximately 4 s for 30 min gradients and 7 s for 120 min gradients'  
  _Reason:_ Uses 'approximately' without a clear comparator establishing a bound; describes default parameter calibration target rather than a verified performance bound.
