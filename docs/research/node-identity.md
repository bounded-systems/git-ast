# Stable code identity: construct vs compute vs resolve

> A literature synthesis for git-ast's node-identity work (trust ledger row 8.1d).
> Produced by a fan-out, adversarially-verified deep-research pass — **25 claims
> verified, 0 refuted** (all unanimous 3-of-3), across 15 primary sources. This is
> the research anchor for the node-identity epic; it complements the design essay
> in the [README](../../README.md#on-stable-node-identity-the-hard-part).

## TL;DR

Establishing "is this the same node across versions?" splits into three strategies,
trading **exactness** against **cost**:

| Camp | Exactness | Cost / requirement | git-ast |
|---|---|---|---|
| **Construct** (Unison) | provably exact; zero-heuristic renames | abandon text files for an AST database | refused by design |
| **Construct / CRDT** (Kleppmann) | machine-proven (Isabelle/HOL) | editor must emit ID-tagged operations | no editor control |
| **Compute** (GumTree) | irreducibly heuristic (NP-hard with moves) | storage-agnostic; post-hoc on plain text | **git-ast's home** |
| **Resolve** (Kythe / SCIP / stack-graphs) | exact-ish for *bindings* | a name resolver / semantic index | unbuilt frontier |

**git-ast's honest 🟡 on 8.1d is rigorously correct:** a tool that keeps plain text
as source of truth and recovers identity *post hoc* cannot be provably correct for
the renamed-and-edited case, because optimal move-aware tree diff is NP-hard.
Provable identity exists — but only by adopting a storage/editing model git-ast
deliberately does not (content-addressing, or editor-cooperated CRDT IDs).

## 1. Construct — identity assigned at birth

**Unison.** A definition's identity *is* a hash of its syntax tree, where bound
variables are **alpha-normalized to positional indices** and **every dependency is
replaced by its own hash** — a Merkle structure over the dependency closure. So a
transitive edit ripples the hash upward through all dependents. Crucially, **names
are separately-stored metadata that do not affect the hash**: renaming only updates
a name→hash mapping, and *"the definition associated with a hash never changes."*
Renames are therefore identity-preserving **by construction, with zero heuristics**,
and non-breaking (call sites bind by hash, not name).

The cost is load-bearing: *"code is stored as its AST in a database"* — precisely the
plain-text-as-truth that git-ast keeps. Construct's exactness is a *direct
consequence* of not storing canonical text, so it is out of reach for a git-native
text-first tool by definition.
*Sources:* unison-lang.org/docs/the-big-idea, github.com/unisonweb/unison.

**Kleppmann's replicated-tree move CRDT** is the collaborative-editing variant: every
node gets a durable globally-unique ID and is moved *by ID* (not path), so identity
survives arbitrary concurrent reparenting. Convergence, single-parent, and acyclicity
are **mechanically proven in Isabelle/HOL** (`Move.thy`, `Move_Acyclic.thy`,
`Move_SEC.thy`). This is genuine provable correctness — but *only because the editor
cooperates* by emitting ID-tagged operations. (IDs are `(timestamp, replica)` pairs,
not literal UUIDs.)
*Source:* martin.kleppmann.com/papers/move-op.pdf.

## 2. Compute — identity recovered after the fact (git-ast's camp)

**GumTree** (Falleri et al., ASE 2014) is exactly git-ast's strategy: a **top-down
phase** anchoring the biggest isomorphic (unmodified) subtrees, then a **bottom-up
phase** propagating mappings to containers (functions, classes) and recovering
matches in leftover code — producing **move-aware edit scripts** (insert/delete/
update/move) aligned to syntax, not text lines. Renames surface as `update` actions
on node labels.

Mapping to git-ast: our Merkle subtree exact-matching ≈ GumTree's **top-down**
anchoring; our statement-level edit script *approximates* the **bottom-up** container
propagation. "Real GumTree" would add explicit **MOVE** actions and cross-container
recovery.

**Why it's irreducibly heuristic** (and why 8.1d is honestly 🟡): optimal tree edit
distance is **O(n³) without moves** (Demaine et al.) and **NP-hard with moves**
(Shapira & Storer, *Edit Distance with Move Operations*, CPM 2002). GumTree is an
explicit greedy O(n²) heuristic. Any post-hoc, text-first tool inherits this
NP-hardness — so similarity heuristics (our Sørensen–Dice over subtree-hash
multisets, greedy matching) are not a weakness specific to git-ast; they are the only
tractable option in the compute camp.
*Sources:* hal.science/hal-01054552, github.com/GumTreeDiff/gumtree.

**Difftastic** (named in the question) turns out to use a *different* algorithm family
than GumTree: it models the two syntax trees as one graph and finds a minimal-cost
route via **Dijkstra shortest-path** — worth studying separately.
*Source:* difftastic.wilfred.me.uk/tree_diffing.html.

## 3. Resolve — binding/use-site identity (under-covered; needs its own pass)

Kythe (`VName` 5-tuple: signature/corpus/root/path/language — "identity is a vector"),
SCIP/LSIF monikers (Sourcegraph; definition vs reference identity, cross-repo
stability), and GitHub stack-graphs / `semantic` (incremental cross-file name
resolution) target **binding identity**. **This camp was not substantiated by the
verified claim set** in this pass (the verification budget went to construct/compute),
so its findings are explicitly *unanswered here* and warrant a dedicated research
pass. Of the three, stack-graphs is the most likely direct borrow for git-ast's
binding-identity frontier (incremental, file-local resolution that composes).

## What git-ast should borrow next (ranked)

1. **Alpha-equivalence hashing** — Maziarz, Ellis, Lawrence, Fitzgibbon & Peyton
   Jones, *Hashing Modulo Alpha-Equivalence* (PLDI 2021): an **O(n·log²n)** algorithm
   to hash a syntax tree robust to bound-variable renaming (prior art was O(n²)),
   proven to retain low collision probability. **Highest-leverage, lowest-risk
   borrow:** it upgrades git-ast's *exact* matching layer to recognize two functions
   identical *up to local variable renaming*, cheaply. (Caveat: probabilistic, not
   absolute.) *Source:* arxiv.org/pdf/2105.02856.
2. **Refactoring-aware differencing** — Alikhanifard & Tsantalis (TOSEM 2024): build
   the diff from *refactoring instances + matched declarations* rather than raw tree
   shape; **considerably higher precision/recall (esp. on refactoring commits)** at
   comparable speed, operating commit-to-commit on declarations (git-ast's interface).
   (Caveat: built on the Java-only RefactoringMiner; porting to tree-sitter grammars
   is an open question; figures are self-reported on an author benchmark.)
   *Source:* arxiv.org/pdf/2403.05939.
3. **Real GumTree bottom-up phase** — explicit move detection + container propagation.

## Out of reach without changing the storage model

Provable, zero-heuristic identity for the changed-and-renamed case. It requires either
content-addressing (Unison) or editor-cooperated CRDT ID-stamping (Kleppmann) — both
of which abandon "plain text files are the source of truth," git-ast's founding
constraint. The honest 🟡 is the right grade.

## Open questions (next research)

1. A proper **resolve pass** (Kythe / SCIP / stack-graphs) for binding/use-site
   identity — which is most borrowable, at what implementation cost.
2. Can refactoring-aware **declaration matching** port to tree-sitter without
   RefactoringMiner's Java dependency, and does it generalize across grammars?
3. A **hybrid**: persist computed node IDs as out-of-band `git notes` seeded once and
   maintained heuristically — how robust is such attribution across `rebase`/`squash`?
4. How does **difftastic's** Dijkstra-on-a-graph approach concretely compare to
   git-ast's statement-level edit script?

## Method note

This synthesis was produced by a deep-research harness: the question was decomposed
into 5 angles, searched in parallel, 15 sources fetched, 72 falsifiable claims
extracted, and the top 25 adversarially verified (3-vote, majority-refute-to-kill).
**All 25 survived (unanimous).** Lower-confidence items (the comparative framework
itself; the resolve camp) are flagged inline above.
