# The identity index: a Merkle trie of canonical subtrees

> The **keystone** note. [`node-identity.md`](./node-identity.md) (compare),
> [`storage-model.md`](./storage-model.md) (persist), and
> [`ci-attestation.md`](./ci-attestation.md) (verify) all sit on **one** substrate:
> a content-addressed **Merkle trie/DAG** of canonical subtrees, keyed by a chosen
> equivalence hash. This note names that substrate.

## From pairwise identity to an index

Identity in git-ast today is **pairwise**: `match` asks *"is node X the same as node
Y?"* across two versions. That is O(versions) comparison and answers one question at a
time. The substrate idea is to make identity an **index**:

> Store every distinct canonical subtree **once**, content-addressed. Looking up a
> node returns its whole **equivalence class** — every node that is "the same" across
> the repo and history — in one hop, not an O(n²) sweep.

This is a **Merkle DAG**: each node is hashed from its children's hashes plus its own
kind/content (exactly git-ast's existing `hash_subtree`), so identical subtrees
*collide* into one stored node and structural sharing is automatic.

## The "(in some way)" is the key you index by

"Nodes that are the same" is not one relation — it is a family, selected by **which
hash you key the trie by**:

| Key the trie by… | "Same" means… | Exact? | git-ast has the hash |
|---|---|---|---|
| `content_hash` | byte-identical canonical form (incl. names) | exact | ✅ |
| `shape_hash` | same body, any name — the **rename class** | exact | ✅ |
| alpha-normalized hash | same modulo bound-variable renaming | exact | ⬜ (PLDI'21 borrow) |
| subtree-multiset / LSH | *similar* — renamed **and** edited | **heuristic** | partial — *not a trie op* |

The first three are **exact equivalence classes by construction** — equal hash ⇒ same
trie node, zero heuristics. The fourth (genuine similarity) is **not** a trie
operation; see the boundary below.

## What git-ast already computes — and discards

`printer::hash_subtree` / `subtree_hashes` already produce the Merkle subtree hashes
for a function… then drop them after a single `match`. **The index is just persisting
those as a shared DAG.** This is the move from *recompute-pairwise-each-time* to
*index-once, query-forever* — the same shift `storage-model.md` argues for, here made
concrete as a data structure.

## What the index buys (beyond comparison)

- **Structural sharing / dedup** — identical subtrees stored once. And *verification*
  dedups too: attest a shared helper **once** and every occurrence inherits it
  (`ci-attestation.md`'s payoff, realized by the index).
- **Equivalence-class queries** — "everything structurally identical to this," clone
  detection, "all call-shapes like this" become a lookup, not a sweep.
- **The Merkle ripple** — change a shared subtree and exactly the parents containing it
  change hash. You know the precise blast radius (incremental CI, impact analysis).
- **The deep/transitive hash for free** — a trie node's hash *is* its deep content,
  which is exactly what `ci-attestation.md` showed semantic checks need to be sound.

## The honest boundary: exact + alpha for free, fuzzy stays heuristic

A Merkle trie gives **exact and alpha-equivalence classes by construction**. It does
**not** group *similar-but-not-identical* nodes — the renamed-**and**-edited case is
still the NP-hard similarity problem from [`node-identity.md`](./node-identity.md) §2,
now layered *over* the trie's leaves rather than run pairwise.

So the index's real virtue is that it **quarantines the heuristic**: everything that
is exactly- or alpha-equivalent is handled by construction and shared, and only the
genuinely-fuzzy remainder pays the heuristic cost (and only against a small candidate
set — the trie narrows it). The honest 🟡 on node identity is unchanged; the index
just shrinks the surface the heuristic has to cover.

## Convergence: this is the one substrate under all four notes

- **Construct camp.** A Merkle DAG of definitions where alpha-equivalent terms collide
  *is* Unison's codebase. The index is the construct-camp storage, realized for
  git-ast over plain text via the existing clean filter — without Unison's editor
  lock-in (we re-derive trie membership at `git add`, not at edit time).
- **Storage model.** [`storage-model.md`](./storage-model.md)'s Dolt proposal *is*
  this index: **prolly trees are Merkle search tries** — content-addressed,
  structurally-sharing, versioned, mergeable. "A Merkle trie for identity" and "store
  the canonical AST in Dolt" are the **same substrate** approached from two directions.
- **Verification.** [`ci-attestation.md`](./ci-attestation.md) keys attestations by a
  trie node's hash, so a verdict is shared across every occurrence and the deep hash it
  needs falls out of the index.
- **Comparison.** [`node-identity.md`](./node-identity.md)'s exact/shape/alpha matching
  becomes index membership; only fuzzy matching stays a separate, narrowed layer.

One content-addressed Merkle index of canonical subtrees, keyed by a chosen
equivalence hash, is the foundation; compare / persist / verify are its three faces.

## Recommendation / next steps

1. **Persist `subtree_hashes` instead of discarding them** — a `subtrees(hash PK,
   kind, child_hashes, …)` store plus an `occurrences(subtree_hash, file, node_path,
   commit)` index. (In Dolt, per `storage-model.md`, this is two tables; the prolly
   tree gives the Merkle structure for free.)
2. **Expose equivalence-class queries** — `git ast same <file:node>` → all occurrences
   in the current tree (and, with history, across commits); a clone-detection report
   falls out.
3. **Add the alpha-normalized key** (PLDI'21 borrow, `prx-zoc7`) to upgrade exact
   classes to alpha-equivalence.
4. **Layer similarity over the trie leaves**, not over raw nodes — the trie supplies
   the candidate set, the heuristic ranks it.

## Open questions

- **Granularity of trie nodes** — every CST node, or only "interesting" ones
  (statements, items, expressions)? Hashing every token node is sound but heavy; a
  cutoff trades index size against query resolution.
- **History dimension** — is the index per-commit (rebuilt) or accumulated across
  history (one DAG spanning all commits)? The latter gives "every version of every
  subtree ever" but needs the `storage-model.md` persistence story and its
  rebase/squash robustness answer.
- **Identity vs. naming** — the index gives structural classes; mapping a stable
  *human* name onto a class over time (Unison's name→hash metadata layer) is a
  separate concern, and the seam where the `resolve` camp ([`node-identity.md`](./node-identity.md) §3) re-enters.
