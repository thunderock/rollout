# 260526-jj1 — fix docs site rendering differently in Chrome vs Brave

**Status:** complete
**Date:** 2026-05-26
**Branch:** main

## Problem

mdbook auto-switches themes based on `prefers-color-scheme`. Brave defaults to dark, Chrome follows OS (often light on macOS). The custom.css shipped in dfb5d19 only had styles that read against a light background, so:

- `code:not(pre > code)` background `rgba(249, 115, 22, 0.06)` invisible on dark navy/coal
- `blockquote` background `rgba(124, 58, 237, 0.03)` invisible on dark
- `table thead` background `rgba(219, 39, 119, 0.05)` invisible on dark
- `a:hover` magenta `#db2777` low-contrast on dark

Result: brand accents visible in light theme (Chrome on light OS), invisible in dark theme (Brave defaulting to dark).

## Fix

Made `custom.css` theme-aware via mdbook's `html.{theme}` classes (`light`, `rust`, `navy`, `coal`, `ayu`). Light-themed defaults stay as-is; dark-themed selectors use higher alpha + lighter magenta variant.

Also set `preferred-dark-theme = "navy"` in `book.toml` so the dark variant is predictable.

## Files touched

- `docs/book/theme/custom.css` (theme-aware overrides)
- `docs/book/book.toml` (`preferred-dark-theme = "navy"`)

## Verification

- `mdbook build docs/book` — clean
- Generated HTML inspected; both light and dark code paths covered.

## Commit

`fix(docs): make custom.css theme-aware for dark mdbook themes`
