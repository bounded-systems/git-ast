# Storage model as the identity layer: a versioned structured store (Dolt)

> A design note, companion to [`node-identity.md`](./node-identity.md). That survey
> concluded that *provable, zero-heuristic* identity for the changed-and-renamed
> case is out of reach for a text-first tool — it needs content-addressing (Unison)
> or editor-cooperated CRDT IDs (Kleppmann), both of which abandon "plain text files
> are the source of truth." This note argues a **versioned structured store** changes
> the *regime* even though it does **not** beat the underlying NP-hardness — and that
> [Dolt](https://www.dolthub.com/) is the natural substrate because it is already our
> stack (beads runs on Dolt; `dolt-box` exists).

## The distinction that makes this work

The "out of reach" result is about the **exactness of the computation** —
reconstructing identity from two text snapshots is NP-hard with moves
(node-identity.md §2), *regardless of where the answer is stored*. A storage model
does not change that. What it changes is **where identity lives and how long it
lives**: from *"recompute a heuristic each time, with nowhere durable to put it"* to
*"compute once at commit, then persist, merge, and query."* Three concrete wins,
each mapping to an open question from the survey:

1. **Persistence done right** (survey open-Q #3). git-ast today follows the essay's
   *"identity is computed, not stored"* — `blame` recomputes from history every run.
   The survey flagged `git notes` as a weak persistence layer (keyed to commit SHAs;
   `squash`/`rebase` collapse them ambiguously). **Dolt is git-for-data with real
   structured merge** — a versioned, branchable table of
   `node_id → (content_hash, name, lineage, first_seen_commit)` that *merges* the way
   code does. Strictly better than notes for durable attribution.

2. **Anchoring → compute-once, not recompute-always.** With the last committed AST
   persisted, each `git add` only matches the **delta** against the stored prior —
   the heuristic runs on what actually changed, and identity *accumulates* instead of
   being re-derived from scratch on every `blame`/`match`. Content-addressed subtree
   hashing stays the lever; the fuzzy step is confined to genuinely-edited nodes.

3. **Cross-file / use-site index** (survey open-Q on binding identity). A single-file
   `inspect` cannot do cross-file matching or use-site tracking. A Dolt **table is a
   whole-repo index** — *"where is node X referenced"* becomes a query. This hands
   part of the RESOLVE-camp frontier (binding/use-site identity) to the store.

## Two architectures

### A. Side-store (hybrid) — keeps the founding constraint
Text files remain the source of truth; Dolt holds **derived** identity/lineage. This
does **not** violate *"text is truth"* — Dolt is an index built by the clean filter,
discardable and rebuildable. The pragmatic, honest first step. Buys persistence (1),
anchoring (2), and the cross-file index (3) without changing what git stores.

### B. AST-in-Dolt — the Unison-flavored move, via the existing seam
git-ast's **clean/smudge filter already canonicalizes** text ↔ stored form. The
stored form *could be Dolt rows* — one row per AST node, primary key = a stable
ID/content hash — with text as the **smudge projection**. Then identity is
*by construction* for anything flowing through the filter; a rename is an
`UPDATE name` with the key unchanged (Unison's property, realized in SQL rather than
in a bespoke codebase, and without Unison's editor lock-in). Dolt's cell-level
versioning + prolly-tree (Merkle) storage gives content-addressing for free.

**The honest catch** (the essay's point): a plain-text edit that *bypasses* the
filter has no embedded IDs, so the next `git add` must run **one compute/match step
at the boundary** to re-anchor IDs against the stored prior AST. So architecture B is
*"compute-once-at-commit, persist-durably"* — much closer to construct than today,
but not fully provable, because the text-editing surface is uncooperative by design.

## What it does and does not buy

| | Today (compute) | + Dolt side-store (A) | + AST-in-Dolt (B) |
|---|---|---|---|
| Exactness of changed+renamed match | heuristic | heuristic | heuristic *at the boundary only* |
| Durable, mergeable attribution | ✗ (recompute) | ✓ | ✓ |
| Cross-file / use-site index | ✗ | ✓ | ✓ |
| Identity stable across history rewrites | ✗ | ✓ (Dolt merge) | ✓ |
| Keeps "text is the source of truth" | ✓ | ✓ | partial (text = projection) |
| Beats NP-hardness | — | no | no |

**Bottom line:** Dolt is the genuine *middle path* between compute and construct. It
will not make the changed-and-renamed case provably correct (NP-hardness is
storage-independent), but it moves git-ast from *recompute-heuristic-each-time* to
*compute-once-at-commit, persist-durably, merge-structurally, query-across-repo*.

## Why Dolt specifically
- **Already our substrate** — beads runs on Dolt; `dolt-box` exists. Not a new
  dependency, the platform we already operate.
- **git-for-data merge semantics** are the right shape for "identity that travels
  with history" — branch, diff, merge, blame on the *identity table itself*.
- **Content-addressed under the hood** (prolly trees) — structurally aligned with
  git-ast's Merkle subtree hashing.

## Recommendation / next steps
1. **Start with architecture A** (side-store). It answers survey open-Qs #1
   (persistence) and the cross-file axis with the least commitment, and is reversible.
2. Define the schema: `nodes(node_id PK, kind, name, content_hash, shape_hash,
   first_seen_commit, last_changed_commit, file_path)` + a `refs` table for use-site
   edges (later).
3. Wire the clean filter to upsert into Dolt on `git add`, matching the delta against
   the stored prior AST (reuse `identity::match_defs`).
4. Re-point `git-ast blame` to read the persisted lineage (fall back to on-demand
   compute when the store is absent — keeps the current behavior as default).
5. Treat **architecture B** as a separate, later spike once A proves the value.

## Open questions
- How robust is the Dolt identity table across `rebase`/`squash`/force-push — does
  its structured merge actually resolve cleanly, or does it need a reconcile step
  (cf. the beads dolt-reconcile experience)?
- Schema for **use-site / binding** edges — depends on the pending RESOLVE research
  pass ([`node-identity.md`](./node-identity.md) §3; bead `prx-zoc7`).
- Does architecture B's boundary re-anchor step compose with the clean filter's
  fail-closed contract (unparseable input rejects the commit)?
