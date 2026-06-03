> **Experimental PDF source.**
> PDF extraction is experimental. pdftotext-mangled column breaks can defeat the validator's local-binding check. Inspect the extracted text in dry-run mode before trusting a non-dry-run extraction.

# Extraction summary

Source: `preprint:rustims-v1-covered-6dadf069`  
Source sha256: `bc94017677e1a73da65ed66daee287d74f4188667fa67f411f64bd52496f3b21`  
Extractor: `claude-opus-4-7` (2026-06-03T12:18:19Z)  


## Accepted claims (0)

_No claims extracted. Default-deny framing rejected all candidates._

## Rejected candidates (13)


### `bound_not_stated` (3)

- page-6 lines 100-102: 'DIA-NN v1.9 showed elevated FDR in the lower-complexity datasets but converged below 1% at higher complexities'  
  _Reason:_ The comparator 'below 1%' applies only at unspecified 'higher complexities', not stated cleanly across the benchmark; partial/conditional bound.
- page-9 line 237: 'PEAKS-XPro reported real FDRs of 1.8% (10k) and 1.15% (100k)'  
  _Reason:_ These are reported point values with no comparator/inequality.
- page-9 lines 238-240: 'FragPipe identified 3,617 (36.8%) and PEAKS-XPro 3,233 (32.9%) of 9,825 fragmented peptides'  
  _Reason:_ Point-value identification rates with no inequality comparator stated.

### `comparator_bound_to_wrong_subject` (2)

- page-6 lines 105-107: 'we observed 3-4% for Spectronaut, up to 5% for DIA-NN v1.8, and 1.5-2% for DIA-NN v1.9 and v2.0'  
  _Reason:_ These are FDR observations for third-party tools (Spectronaut, DIA-NN) on simulated data, not bounds on the paper's own system (timsim). Extraction would attach a bound to a benchmarked tool, which is outside the paper's claim subject.
- page-6 line 107: 'FragPipe was the only tool controlling FDR at the expected value of 1%'  
  _Reason:_ Claim is about FragPipe (third-party tool), not timsim itself; the paper's subject is the simulator. Also lacks an explicit inequality.

### `hedged_qualitative_only` (1)

- page-6 lines 99-101: 'DIA-NN v2.0, Spectronaut v20, and both tested FragPipe versions (v22 and v23) achieved a true FDR close to the nominal 1% threshold on ion level'  
  _Reason:_ "Close to the nominal 1% threshold" is not a directional comparator with a bound; no exact numeric inequality stated.

### `metric_not_named` (6)

- page-3 abstract: 'a 0.65 site-probability cutoff as an optimal tradeoff between sensitivity and false localization'  
  _Reason:_ This is a recommended threshold/parameter, not a measured metric with a comparator bound on a system's performance.
- rustims-v1-hla-fragpipe-fdr-10k: 'FragPipe achieved 0.91% and 1.16% on the same datasets'  
  _Reason:_ validator rejected tolerance for 'rustims-v1-hla-fragpipe-fdr-10k': metric token 'true FDR (HLA-I 10k thunder-dda-PASEF)' not present in source_span
- rustims-v1-maxquant-peak-matching-error: "MaxQuant's peak matching error rate rose sharply, reaching up to 30% in the 7.5 min, 150,000-peptide setup"  
  _Reason:_ validator rejected tolerance for 'rustims-v1-maxquant-peak-matching-error': metric token 'peak matching error rate (7.5 min, 150,000-peptide dda-PASEF)' not present in source_span
- rustims-v1-mbr-contribution: 'In our simulation, MBR contributed to 10–20% of identifications per run'  
  _Reason:_ validator rejected tolerance for 'rustims-v1-mbr-contribution': metric token 'MBR share of identifications per run' not present in source_span
- rustims-v1-mbr-contribution: 'In our simulation, MBR contributed to 10–20% of identifications per run'  
  _Reason:_ validator rejected tolerance for 'rustims-v1-mbr-contribution': metric token 'MBR share of identifications per run' not present in source_span
- rustims-v1-overall-identification-errors-mbr: 'Overall identification errors remained below 1% across both tools'  
  _Reason:_ validator rejected tolerance for 'rustims-v1-overall-identification-errors-mbr': metric token 'overall identification error (MBR benchmark, both tools)' not present in source_span

### `ranking_language_only` (1)

- page-6 lines 120-122: 'DIA-NN consistently detected precursors at lower simulated intensities than other tools'  
  _Reason:_ Ranking statement without a numeric bound.
