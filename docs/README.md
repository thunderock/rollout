# docs/

Project documentation. Two kinds of docs live here:

- **`design-principles.md`** — the *why* behind every architectural choice. Read this before designing anything new.
- **`specs/`** — one file per component. Each is an implementation contract. Read the relevant spec before implementing or modifying a component.
- **`adr/`** — Architecture Decision Records. Created when a decision warrants a paper trail (irreversible, controversial, or non-obvious).

If you're new to the project, the recommended reading order is:

1. `/README.md`
2. `/AGENTS.md`
3. `/SKILLS.md`
4. `/ARCHITECTURE.md`
5. `/ROADMAP.md`
6. `docs/design-principles.md`
7. `docs/specs/00-overview.md`
8. … then whichever spec is relevant to your task.

## Writing docs

- Code-style docs (cargo doc, mypy stubs) are generated from source. Don't hand-write them.
- High-level docs (this directory) are hand-written and reviewed.
- Every spec must follow the skeleton from `specs/00-overview.md`. PRs that change spec structure should update the overview accordingly.

## Generated docs (not in repo)

- `docs.rs` hosts crate docs (cargo doc).
- An mdBook site is built from this directory at release time.
- A separate Python docs site is built via mkdocs.
