# Roadmap

> **Status:** design stage. Phases below are intent, not delivered work. See the
> [README](../README.md#project-status) for current reality.

The roadmap reflects the staging in
[`planning/strategy-memo.md`](./planning/strategy-memo.md) and the MVP boundaries
in [`planning/scope.md`](./planning/scope.md).

## Phase 0 — Skeleton (current)

- Subcommand entry point (`filter-process`, `diff-driver`, `merge-driver`) that
  compiles and runs with placeholder logic.
- Documentation of the architecture and the core open problems.

## Phase 1 — Round-trip for one language

- Parse Rust with Tree-sitter; serialize the tree as the stored blob.
- Deterministic pretty-printer so `smudge(clean(source))` is canonical and
  stable (idempotent).
- Real clean/smudge filter speaking Git's long-running filter protocol.

## Phase 2 — Structural diff

- Read-only AST diff driver: suppress formatting-only changes, show structural
  edits (add / delete / update). This is the first user-visible win.

## Phase 3 — Structural merge

- 3-way tree merge with conflict-marker fallback via the merge driver.

## Phase 4 — Node identity (the hard part)

- Stable identity for tree nodes across versions, enabling move detection and
  refactor-aware history. Explicitly **out of scope** for the MVP; see
  [`planning/scope.md`](./planning/scope.md).

## Later

- Additional languages, platform/CI integration (the
  [mirror-repo workaround](./architecture/clean-smudge-filters.md) for hosts that
  don't run local filters), and semantic blame
  ([`future-directions.md`](./future-directions.md)).
