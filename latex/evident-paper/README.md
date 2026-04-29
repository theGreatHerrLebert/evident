# EVIDENT Paper Draft

Working LaTeX draft for the EVIDENT manuscript.

## Build

```bash
make
```

This expects `latexmk` and a reasonably complete LaTeX install. The fallback
command is:

```bash
pdflatex main.tex
bibtex main
pdflatex main.tex
pdflatex main.tex
```

## Draft Position

The paper should not read as a paper about a repository. It should argue for a
missing trust layer in AI-assisted scientific software:

> The unit of trust is not code, but a claim about code.

EVIDENT is the proposed lightweight workflow and manifest structure. Proteon is
the main realistic case study because it has many scientific-computing claims,
external oracles, tolerances, reproducible commands, and visible gaps.

Before submission, add one small fully sealed case study with pinned versions,
committed artifact, replay command, and populated verification metadata.

