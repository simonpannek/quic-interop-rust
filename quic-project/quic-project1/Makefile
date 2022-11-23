PANDOCSRC := $(wildcard *.md)
PANDOCPDF := $(PANDOCSRC:.md=.pdf)


.phony: all 
all: pdf 

pdf: $(PANDOCPDF) 
	
%.pdf: %.md pandoc_template.yaml biblio.bib
	pandoc pandoc_template.yaml $< -o $@ --pdf-engine=pdflatex --bibliography=biblio.bib --default-image-extension=pdf -V urlcolor=cyan --csl=ieee.csl --citeproc

clean:
	rm -f $(PANDOCPDF) 
