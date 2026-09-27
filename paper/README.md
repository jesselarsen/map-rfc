# Paper: Mutual Attestation for Delegated Agent Work

ACM `acmart` (sigconf, `nonacm` preprint mode). The source is
self-contained: `main.tex`, `sections/*.tex`, `references.bib`, `figs/*.pdf`.

## Build

This machine has no TeX install, so the PDF has **not been compiled yet**.
Either:
- **Overleaf:** upload the `paper/` directory, or `paper-overleaf.zip`,
  as a new project. `acmart` is preinstalled there.
- **Local TeX Live:** run `latexmk -pdf main.tex`. It needs
  `texlive-publishers` for `acmart`.

`python paper/lint_tex.py` runs static checks without TeX:
- refs and labels, and cite keys against the `.bib`;
- brace and environment balance;
- figures present;
- non-ASCII characters, and unescaped `_ & #`.

It was validated against planted errors. It does not replace compiling.

## Provenance of every number

- **Evaluation numbers** come from `eval/results/5bc7702/REPORT.md`,
  `MINIMAX.md` and `B2L.md`. All three are generated from the committed
  per-seed CSVs, by `analyze.py`, `minimax.py` and `b2l.py` respectively.
  `B2L.md` is exploratory.
- **Implementation numbers** come from the tree at the commit that adds
  this directory: line counts via `git ls-files`, and test counts from
  `cargo test` (167) and `go test -v` (78).
- **Byte sizes** come from the test vectors (`spec/test-vectors`).
- **Benchmarks** come from `eval/results/5bc7702/bench/`.

## Figures

To regenerate:
```bash
python eval/analysis/make_figs.py eval/results/5bc7702 --out paper/figs
python paper/figs/make_flow.py
```

## Before submission (author decisions)

1. **Affiliation country.** `main.tex` has `\country{}` with a TODO.
2. **Repository visibility.** `jesselarsen/map-rfc` is private. The paper
   says the artifact "will be released" at tag `paper-v1`. Make it
   public, or archive it on Zenodo for a DOI, before posting, and update
   Appendix A with the URL/DOI.
3. **arXiv category and endorser (D7).** The plan is cs.CR primary,
   cross-listed to cs.MA and cs.NI. First-time submitters to cs.CR need
   an endorsement.
4. **Compile and proofread the PDF.** Check the width of Table 3 (the
   related-work table spans both columns) and figure sizing.
