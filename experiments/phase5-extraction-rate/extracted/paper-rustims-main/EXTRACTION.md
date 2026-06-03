> **Experimental PDF source.**
> PDF extraction is experimental. pdftotext-mangled column breaks can defeat the validator's local-binding check. Inspect the extracted text in dry-run mode before trusting a non-dry-run extraction.

# Extraction summary

Source: `preprint:rustims-v1-covered-6dadf069`  
Source sha256: `bc94017677e1a73da65ed66daee287d74f4188667fa67f411f64bd52496f3b21`  
Extractor: `claude-opus-4-7` (2026-06-03T13:52:39Z)  


## Accepted claims (1)

- **rustims-maxquant-peak-matching-error-7p5min** — MaxQuant peak matching error reaches up to 30% in 7.5 min, 150,000-peptide setup  
  Tolerances: 1; subject aliases: ['MaxQuant']

## Rejected candidates (21)


### `bound_not_stated` (6)

- Abstract: 'identifying a 0.65 site-probability cutoff as an optimal tradeoff between sensitivity and false localization'  
  _Reason:_ This is a recommended cutoff, not an empirical bound on a measured metric of a system.
- page 6, lines 106-107: 'we observed 3-4% for Spectronaut, up to 5% for DIA-NN v1.8, and 1.5-2% for DIA-NN v1.9 and v2.0'  
  _Reason:_ These are observed ranges rather than stated upper/lower bound comparators in the form required.
- page 6, line 110: 'in DIA-NN v1.9 and v2.0, the spurious oxidized form replaced the correct unmodified identification in 99% and 86% of cases'  
  _Reason:_ Reports point values without a comparator (no '<', '>', 'at least', 'at most' phrasing on these percentages).
- page 6: 'FragPipe reached 90% sensitivity of DIA-NN'  
  _Reason:_ Point value without comparator.
- page 9, line 224: 'MBR contributed to 10–20% of identifications per run'  
  _Reason:_ Range value without a strict comparator.
- page 9: 'Each tool also contributed 10–15% unique identifications'  
  _Reason:_ Vague range, no comparator.

### `comparator_bound_to_wrong_subject` (3)

- Abstract, page 3: 'several dia-PASEF workflows control FDR near the nominal 1% threshold at stripped-sequence level but exhibit inflated true FDR (3–5%) when modified peptidoforms are considered'  
  _Reason:_ Subject is generic 'several dia-PASEF workflows', not a specific named tool. The 3-5% range is also not tied to a single subject with a comparator.
- page 6, line 118: 'all DIA-NN versions identified the largest number of precursors, up to 10% more than FragPipe'  
  _Reason:_ 'Up to 10% more' is a vague upper bound on a difference metric; not a cleanly tied claim of (metric, comparator, value, subject).
- page 8, line 206: 'Overall identification errors remained below 1% across both tools'  
  _Reason:_ Subject is 'both tools' generically; not tied to a single named subject for which the validator can match a source_span.

### `hedged_qualitative_only` (3)

- page 6, ~line 99: 'DIA-NN v2.0, Spectronaut v20, and both tested FragPipe versions (v22 and v23) achieved a true FDR close to the nominal 1% threshold on ion level'  
  _Reason:_ 'Close to' is not a strict comparator with a specific numeric bound.
- page 6: 'DIA-NN v1.9 ... converged below 1% at higher complexities'  
  _Reason:_ Conditional on 'higher complexities' (not precisely defined) and no specific measurement value tied to a clear comparator+subject in a way suitable for extraction.
- page 6: 'FragPipe was the only tool controlling FDR at the expected value of 1%'  
  _Reason:_ Qualitative statement without strict comparator and value bound on a specific metric.

### `metric_not_named` (9)

- Abstract: 'match-between-runs produced peak-matching errors of up to 30% under high-density conditions'  
  _Reason:_ Same finding is captured more specifically in body text and attributed to MaxQuant; abstract version is not tool-specific.
- rustims-peaks-xpro-fdr-10k: 'PEAKS-XPro reported real FDRs of 1.8% (10k) and 1.15% (100k)'  
  _Reason:_ validator rejected tolerance for 'rustims-peaks-xpro-fdr-10k': metric token 'real_FDR_10k_HLA' not present in source_span
- rustims-peaks-xpro-fdr-10k: 'PEAKS-XPro reported real FDRs of 1.8% (10k) and 1.15% (100k)'  
  _Reason:_ validator rejected tolerance for 'rustims-peaks-xpro-fdr-10k': metric token 'real_FDR_100k_HLA' not present in source_span
- rustims-fragpipe-fdr-hla: 'FragPipe achieved 0.91% and 1.16% on the same datasets'  
  _Reason:_ validator rejected tolerance for 'rustims-fragpipe-fdr-hla': metric token 'real_FDR_10k_HLA' not present in source_span
- rustims-fragpipe-fdr-hla: 'FragPipe achieved 0.91% and 1.16% on the same datasets'  
  _Reason:_ validator rejected tolerance for 'rustims-fragpipe-fdr-hla': metric token 'real_FDR_100k_HLA' not present in source_span
- rustims-fragpipe-hla-10k-identification: 'Of the 9,825 fragmented peptides in the 10k dataset, FragPipe identified 3,617 (36.8%)'  
  _Reason:_ validator rejected tolerance for 'rustims-fragpipe-hla-10k-identification': metric token 'fraction_of_fragmented_peptides_identified_10k' not present in source_span
- rustims-peaks-xpro-hla-10k-identification: 'PEAKS-XPro 3,233 (32.9%)'  
  _Reason:_ validator rejected tolerance for 'rustims-peaks-xpro-hla-10k-identification': metric token 'fraction_of_fragmented_peptides_identified_10k' not present in source_span
- rustims-fragpipe-hla-100k-identification: 'FragPipe recovered 28,108 (30.5%)'  
  _Reason:_ validator rejected tolerance for 'rustims-fragpipe-hla-100k-identification': metric token 'fraction_of_fragmented_peptides_identified_100k' not present in source_span
- rustims-peaks-xpro-hla-100k-identification: 'PEAKS-XPro 27,182 (29.5%)'  
  _Reason:_ validator rejected tolerance for 'rustims-peaks-xpro-hla-100k-identification': metric token 'fraction_of_fragmented_peptides_identified_100k' not present in source_span
