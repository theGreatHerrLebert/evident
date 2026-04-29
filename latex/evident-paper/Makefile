LATEXMK ?= latexmk

.PHONY: all clean distclean

all: main.pdf

main.pdf: main.tex references.bib figures/evident_workflow.pdf figures/claim_layers.pdf
	@if command -v $(LATEXMK) >/dev/null 2>&1; then \
		$(LATEXMK) -pdf -interaction=nonstopmode -halt-on-error main.tex; \
	else \
		pdflatex -interaction=nonstopmode -halt-on-error main.tex; \
		bibtex main; \
		pdflatex -interaction=nonstopmode -halt-on-error main.tex; \
		pdflatex -interaction=nonstopmode -halt-on-error main.tex; \
	fi

clean:
	@if command -v $(LATEXMK) >/dev/null 2>&1; then \
		$(LATEXMK) -c main.tex; \
	else \
		rm -f main.aux main.bbl main.blg main.fdb_latexmk main.fls main.log main.out; \
	fi

distclean:
	@if command -v $(LATEXMK) >/dev/null 2>&1; then \
		$(LATEXMK) -C main.tex; \
	else \
		rm -f main.aux main.bbl main.blg main.fdb_latexmk main.fls main.log main.out main.pdf; \
	fi
