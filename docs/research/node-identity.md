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

## 3. Resolve — binding/use-site identity

The resolve camp answers a *different* question than compute or construct: not "is
this the same syntax across versions?" but **"which declaration does this name refer
to?"** — distinguishing two same-named functions in different scopes, and tracking a
symbol to its use sites. Three exemplars, ordered by fit for a text-first,
tree-sitter, single-binary tool:

**Kythe** identifies every node in its semantic graph by a **`VName` 5-tuple**
(`signature`, `corpus`, `root`, `path`, `language`) — identity as a *vector*, with
`signature` a per-(corpus,root,path,language) unique string. Definition / reference /
anchor edges encode the graph. The cost is heavy: **per-language indexers wired into
the build system** (it consumes compiler output), so it is a whole-repo, build-coupled
pipeline — the opposite of git-ast's per-file, build-free filter.
*Source:* kythe.io/docs/schema.

**SCIP** (Sourcegraph, the LSIF successor) centers on **human-readable string symbols**
that replace LSIF's opaque numeric IDs and "monikers" — a structured symbol scheme
(`scheme package-manager package-name version descriptors…`) giving stable,
**cross-repository** identifiers. It is a Protobuf format (LSIF was graph-JSON):
reported **~8× smaller and ~3× faster to process**, ~10× faster to produce, and far
easier to author indexers for. But SCIP is still an *index format* produced by
**compiler/type-aware indexers** — it presumes a resolvable build, like Kythe.
*Source:* sourcegraph.com/blog/announcing-scip.

**Stack graphs** (GitHub; extend Visser et al.'s scope graphs) are the standout fit.
Name resolution becomes **graph path-finding**: *"every valid name binding is
represented by a path from a reference node to a definition node,"* validated by a
**symbol stack** (push/pop nodes) that encodes shadowing/precedence. Two properties
make them ideal for git-ast:
- **File-local & incremental.** *"At index time we look at each file completely in
  isolation"* — each file's graph (including unresolved cross-file references) is built
  alone, then graphs are **merged at query time** into one commit-level graph.
  Unchanged files reuse cached graphs; **no whole-program analysis, no build system.**
- **Tree-sitter native.** Graphs are built from tree-sitter CSTs via a declarative
  **graph-construction language** (`stanzas` attached to tree-sitter queries), so
  *"the only language-specific part is the set of graph construction rules for that
  language."*
*Source:* github.blog/2021-12-09-introducing-stack-graphs.

**Verdict for git-ast.** Kythe and SCIP both presume a build/compiler pipeline —
disqualifying for a per-file, text-first filter. **Stack graphs are the borrow:**
file-isolated construction matches git-ast's per-file clean filter exactly,
tree-sitter is already the parser, and the per-language cost is *declarative graph
rules* — structurally the same kind of additive, per-language work as git-ast's
existing per-language printers. It hands over **cross-file matching and use-site
tracking** (the binding axis) without a build dependency.

**Does name resolution make identity more exact?** Partly, and on a *different* axis.
Binding resolution is **exact within a single version** — "which `parse` did you
mean" stops being a guess. That directly helps **disambiguation** (same-named symbols
in different scopes) and **cross-file** matching. But matching binding *structure
across versions* when the name **and** the body changed together is still the same
NP-hard problem from §2 — resolve **complements** compute, it does not replace it.
Net: resolve removes the *within-snapshot* ambiguity; the *across-version* heuristic
remains.

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
4. **Stack-graphs-style binding resolution** (§3) — declarative tree-sitter
   graph-construction rules for the Rust subset, giving file-local cross-file matching
   and use-site tracking with no build dependency. The resolve-camp borrow.

## Out of reach without changing the storage model

Provable, zero-heuristic identity for the changed-and-renamed case. It requires either
content-addressing (Unison) or editor-cooperated CRDT ID-stamping (Kleppmann) — both
of which abandon "plain text files are the source of truth," git-ast's founding
constraint. The honest 🟡 is the right grade.

## Open questions (next research)

1. Can refactoring-aware **declaration matching** port to tree-sitter without
   RefactoringMiner's Java dependency, and does it generalize across grammars?
2. A **versioned structured store** (Dolt) as the persistence + anchoring + cross-file
   index layer — see the companion note [`storage-model.md`](./storage-model.md). This
   subsumes the earlier "persist IDs in `git notes`" question (Dolt has real structured
   merge; notes collapse under `rebase`/`squash`).
3. How does **difftastic's** Dijkstra-on-a-graph approach concretely compare to
   git-ast's statement-level edit script?
4. What is the minimal **stack-graphs graph-construction rule set** for git-ast's Rust
   subset, and how much of the binding axis does it unlock per unit of per-language
   effort?

## Method note

The construct and compute camps (§1–2) were produced by a deep-research harness: the
question was decomposed into 5 angles, searched in parallel, 15 sources fetched, 72
falsifiable claims extracted, and the top 25 adversarially verified (3-vote,
majority-refute-to-kill) — **all 25 survived (unanimous)**. The resolve camp (§3) was
added by a lighter, direct primary-source sweep (Kythe schema, the SCIP announcement,
the GitHub stack-graphs post) after a follow-up harness run hit a transient fetch
rate-limit — single-source rather than triangulated, so treat §3 as well-sourced but
not adversarially cross-verified. The comparative framework itself is reviewer-
constructed, not lifted from one source.
