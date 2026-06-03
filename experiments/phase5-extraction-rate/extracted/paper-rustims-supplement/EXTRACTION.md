> **Experimental PDF source.**
> PDF extraction is experimental. pdftotext-mangled column breaks can defeat the validator's local-binding check. Inspect the extracted text in dry-run mode before trusting a non-dry-run extraction.

# Extraction summary

Source: `preprint:rustims-supplement-df40ee33`  
Source sha256: `cf09b769e3804fbd10c8a6a1454696ca65fc8b37743a6a99d318b1a9d2f0d587`  
Extractor: `claude-opus-4-7` (2026-06-03T12:19:00Z)  


## Accepted claims (0)

_No claims extracted. Default-deny framing rejected all candidates._

## Rejected candidates (6)


### `bound_not_stated` (1)

- Section HeLa Samples (dda-PASEF), page 4: 'minimum precursor intensity of 1,000 and a dynamic exclusion window of 25 frames'  
  _Reason:_ These are simulation configuration values, not empirical performance claims with comparators about the timsim system's behavior.

### `comparator_bound_to_wrong_subject` (4)

- Section 1.5.4 PEAKS: 'Results were filtered at FDR ≤ 0.01 for peptides and −10 log P ≥ 0 for proteins.'  
  _Reason:_ This describes filter thresholds applied within PEAKS configuration for the analysis, not a performance claim about the timsim/rustims artifact being documented.
- Section 'HLA1 Samples (thunder-dda-PASEF)' page 5: 'HLA1 samples were generated starting from a collection of experimentally measured and confidently identified peptides (FDR ≤ 1%)'  
  _Reason:_ The FDR ≤ 1% bound describes filtering of the input experimentally identified peptide set (from a cited prior publication), not a performance claim about timsim itself.
- Section 1.5.2 FragPipe: 'ion level FDR set to 0.01 (default parameter)'  
  _Reason:_ Describes a configuration setting for an external tool (FragPipe), not a performance bound of the timsim artifact.
- Section 1.5.4 PEAKS: 'Peptides were identified with mass accuracy thresholds of 15 ppm for MS1 and 0.03 Da for MS2.'  
  _Reason:_ Configuration parameter for PEAKS analysis, not a measured performance bound of the subject artifact.

### `hedged_qualitative_only` (1)

- Section 2.1.3, page 13: 'the mean central 76% interval width of a sample of curves is approximately 4 s for 30 min gradients and 7 s for 120 min gradients'  
  _Reason:_ Uses 'approximately' and describes a target value used in deriving default parameters (and cites Meier et al.), not a strict numeric bound with comparator on the timsim subject.
