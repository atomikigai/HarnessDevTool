---
name: pdf-oxide
description: Extract text, markdown, images and metadata from PDF files. Use when an agent needs to read a PDF. Produces markdown output with preserved headings, tables, and reading order — optimized for LLM consumption. Triggers: "read this PDF", "extract text from PDF", "convert PDF to markdown", "what does this PDF say".
metadata:
  short-description: PDF to Markdown extraction for LLM pipelines
  version: "0.3.60"
  install: cargo binstall pdf_oxide_cli
---

# pdf-oxide — PDF Extraction for LLM Pipelines

Fastest PDF toolkit (0.8ms/doc, 100% pass rate on 3,830 PDFs). MIT licensed.

## Extract for LLM Consumption

```bash
# Markdown (PREFERRED — preserves headings, tables, reading order)
pdf-oxide markdown paper.pdf
pdf-oxide markdown paper.pdf -o paper.md
pdf-oxide markdown paper.pdf --detect-headings

# Plain text (when structure doesn't matter)
pdf-oxide text paper.pdf
```

## Page Ranges (supported by all commands)

```bash
pdf-oxide markdown paper.pdf --pages 1-10
pdf-oxide text paper.pdf --pages 1,3,7-10
```

## Search Within a PDF

```bash
pdf-oxide search paper.pdf "neural network"
pdf-oxide search contract.pdf "termination|cancellation"   # regex
pdf-oxide search paper.pdf "equation \d+" --json
```

## Metadata

```bash
pdf-oxide info paper.pdf          # page count, version, metadata
pdf-oxide bookmarks paper.pdf     # document outline/TOC
```

## Machine-Readable Output

```bash
pdf-oxide text paper.pdf --json
pdf-oxide search paper.pdf "pattern" --json
```

## Encrypted PDFs

```bash
pdf-oxide markdown protected.pdf --password "secret"
```

## In the Harness

The knowledge base ingestion uses the `pdf_oxide` Rust crate directly via
`PdfDocument::to_markdown()`. This CLI is for agents reading PDFs on-demand
during task execution.

## When NOT to Use

- Scanned PDFs (no text layer) → need OCR first (tesseract)
- Very large PDFs (>500 pages) → process with --pages in batches
