# Git AST: A Language-Aware Git Extension

Git AST provides **language-aware extensions for Git**, leveraging Abstract Syntax Trees (ASTs) instead of traditional line-based diffs. This enhances Git with semantic understanding, leading to more meaningful history, easier merges, and enhanced code consistency.

## Value Proposition

Why use Git AST?

- **Cleaner Diffs:** Focus on meaningful code changes, ignore formatting noise
- **Smarter Merges:** Reduce conflicts caused by code movement or non-competing structural edits
- **Consistent Formatting:** Enforce a canonical code style automatically across your repository

## How It Works

Git AST leverages Git's clean and smudge filters to operate on the structure of your code:

1. **When You Commit:** Source code is parsed into a syntax tree (AST/CST) and stored in Git
2. **When You Check Out:** The stored tree is converted back into consistently formatted source code

This structural approach lets you focus on semantic changes rather than textual differences.

## Getting Started

- [Installation Guide](./docs/getting-started/installation.md) - Set up Git AST in your environment
- [Usage Guide](./docs/getting-started/usage.md) - Learn how to use Git AST in your workflow
- [Documentation](./docs/start-here.md) - Comprehensive documentation

## Documentation

### Core Documentation
- [Project Overview](./docs/overview.md) - Goal, mechanism, and key concepts
- [Roadmap](./docs/roadmap.md) - Project development timeline

### Technical Documentation
- [Architecture Design](./docs/architecture/design.md) - Technical architecture and data flow
- [Clean/Smudge Filters](./docs/architecture/clean-smudge-filters.md) - Details on the Git filter implementation

### Concepts and Reference
- [Key Concepts](./docs/concepts/key-concepts.md) - Detailed explanation of core concepts
- [Glossary](./docs/concepts/glossary.md) - Definition of terms
- [FAQ](./docs/concepts/faq.md) - Frequently asked questions

### Contributing
- [Contribution Guidelines](./docs/contributing/guidelines.md) - How to contribute
- [Development Setup](./docs/contributing/development-setup.md) - Setting up your development environment

For a full documentation overview, see [Documentation Index](./docs/README.md).

## Project Status

**Working clean/smudge round-trip for two languages — JSON and a Rust subset.**
The core pipeline is implemented and runs through real Git:

- `git-ast setup` registers the filter in a repository (routes `*.rs` and
  `*.json`).
- On `git add`, the `clean` filter parses the source and stores its
  **canonical** form; on `git checkout`, `smudge` returns it. Reformatting
  therefore never reaches history — two differently-formatted inputs that parse
  to the same structure produce byte-identical blobs. Canonicalization is
  **deterministic and idempotent**. Rust is canonicalized via Tree-sitter (the
  "Determinism contract" in [`src/printer.rs`](./src/printer.rs), versioned by
  the `(grammar, printer)` pair); JSON via `serde_json`
  ([`src/json.rs`](./src/json.rs) — sorted keys, pretty-printed).
- It speaks Git's real `filter-process` pkt-line protocol, so `git add` /
  `git checkout` / `git diff` all work end to end. See
  [`examples/demo.sh`](./examples/demo.sh).
- `git-ast inspect [FILE]` lists top-level definitions with a
  **formatting-invariant content hash** — a proof-of-concept of the first
  read verb (see "The interface: verbs" below).
- **Structural 3-way merge for JSON.** `git-ast setup` also wires a real merge
  driver ([`src/merge.rs`](./src/merge.rs)): on `git merge`, JSON is merged by
  **structure** — edits and additions to *different* object keys merge cleanly
  even when they touch adjacent lines (where a text merge would conflict); only a
  genuine same-key divergence conflicts. Proven against real `git merge` by the
  cucumber claims suite. (A machine-checked **Lean** proof of the merge's
  soundness is the immediate follow-up — see boundaries below.)

Honest boundaries:

- **JSON is complete; Rust is a (growing) subset.** JSON canonicalization is
  total (any valid JSON round-trips). The Rust pretty-printer now covers
  module-level items — `use`, `const`/`static`, named & unit `struct`s, unit-
  variant `enum`s, `impl` blocks (incl. trait impls) — plus functions, `self`
  receivers, references, generics, struct literals, field access, `let`,
  binary/call/macro expressions, literals, and comments. Both languages are
  **fail-closed**: syntax errors reject the commit, and any *unsupported* Rust
  construct (tuple structs, traits, generic params, enum payloads, lifetimes, …)
  returns an error rather than corrupting code. Widening Rust coverage is additive
  — one more arm per node kind; adding a language is one more arm in the filter's
  per-extension dispatch.
- **Structural merge is JSON-only, and not yet Lean-proven.** The merge driver
  handles `*.json`; the Rust-language structural merge (over the Tree-sitter CST)
  and array element-level merging are later increments. The merge algorithm's
  soundness is exercised by Rust property tests now; a Lean proof (the formal
  half of "backed by Rust *and* Lean") lands next.
- **The diff driver is still a placeholder, and merge does not track node
  identity.** Structural *diff*, and tracking a node through a move or
  rename, depend on the hardest open problem — **stable AST node identity across
  versions** — which this does **not** solve. That problem is described in
  [`docs/planning/scope.md`](./docs/planning/scope.md) and remains out of scope.

## On stable node identity (the hard part)

"Node identity across versions" means being able to say *this* function in
commit N is the same entity as *that* one in commit N+1 — through a move, a
rename, an extract-method — so attribution follows the node, not its line
position. It is what canonical formatting alone does **not** buy you, and it is
the floor under reliable per-line attribution. A few things worth stating
plainly, because they are easy to get wrong:

- **It is heuristic, not exact.** "Is this the same function after a rename
  *and* a body rewrite?" has no ground truth — it is a judgment. You can get it
  very good (a pure move, or a rename with an unchanged body, is near-certain);
  you cannot get it provably correct.

- **Identity is *computed*, not *stored*.** Embedding durable IDs in nodes fails
  the moment a plain text editor touches the file (the IDs aren't there to
  preserve). Because git-ast stores canonical *text*, identity must be derived
  by matching tree N against tree N+1 (GumTree-family algorithms) at the time
  you ask — not carried in the blob.

- **Content-addressed subtree hashing is the lever.** Hash every subtree; an
  unchanged-but-moved node has the *same hash* in both commits and matches for
  free, with zero heuristics. Fuzzy matching is then needed only for the
  subtrees that actually changed — shrinking the uncertain surface to just the
  genuinely-edited nodes.

- **`git notes` are a transport, not the mechanism.** Computing identity needs
  no notes. Notes only matter for *persisting* attribution and carrying it
  across history rewrites — and they do **not** survive rewrites for free: they
  are keyed to commit SHAs, `rebase`/`amend`/cherry-pick copying is per-commit
  and not merge-aware, and **squash collapses several commits' notes
  ambiguously**. Making attribution "move and merge through every rewrite" is
  the hard engineering, not a property notes hand you.

### Identity is a vector, not a scalar

"Is this the same node?" is underdetermined because identity is not one
property but several **independent dimensions** that come apart under different
edits. This is not new: Google [Kythe](https://kythe.io/docs/kythe-storage.html)
models every semantic node as a `VName` — a 5-tuple `(signature, corpus, root,
path, language)` — and states outright that "a node is a d-dimensional vector,
each dimension a scalar fact." Treat node identity the same way: a tuple,
resolved per purpose, not a single key. Stated precisely, splitting each
dimension that is not atomic:

- **Content — shallow vs deep.** *Shallow* content is the node's own normalized
  structure with identifiers alpha-renamed and dependencies abstracted; *deep*
  (Merkle) content folds dependency identities into the hash. They have
  *opposite* stability under change propagation: rename a callee `g` and `f`'s
  shallow content is unchanged while its deep content changes (the Unison
  behaviour — deps by hash). Collapsing the two is a category error — shallow is
  stable but coarse, deep is precise but ripples on any transitive edit.
- **Name — lexeme vs binding.** The surface string (`parseConfig`) versus the
  resolved declaration a use points to (a compiler `DefId`). A rename changes
  the lexeme; the binding persists, and shadowing gives same-lexeme /
  different-binding. Most rename-robustness comes from binding identity, not the
  string — which is why Kythe keys on `signature`, not the name.
- **Definition vs use/call.** A definition and its call sites are *separate*
  dimensions: the def can be stable while callers churn, or the reverse.
  "Track the def" and "track who references the def" are different identities
  with different lifetimes ([SCIP/LSIF](https://github.com/sourcegraph/scip)
  monikers separate them). This is also why the **export surface** is special:
  at an API boundary use-identity becomes a *contract* — semver and
  breaking-change detection key on it — so the otherwise-weak use axis becomes
  the durable one.
- **Location.** Path, offset, sibling order. Weakest (breaks on every move),
  most available (it is what text and git already have). Mostly a tiebreaker.

Two things the single word "identity" hides:

- **Dimensions differ in epistemic cost, not just in what they track.** Content
  and Location are computable from text alone; Name(binding) and use/call need a
  resolver or whole-program analysis. In a no-build, partial-file, or
  multi-language context, half the vector does not exist — *availability*, not
  preference, decides which dimensions you can use.
- **Equivalence is not persistence.** Content identity is *many-to-one*: two
  distinct helpers with identical bodies share a content hash. Key blame on
  content and you *fuse clones* into one false lineage. Content gives
  equivalence classes; "the same entity over time" is a different relation
  (correspondence between two versions, then persistence across N).

### Three ways to establish it — and only one scales

Given the vector, how do you decide two nodes correspond across versions? Three
families, not equal:

1. **By construction** — assign a durable id at birth and have the editor carry
   it. Kleppmann's [replicated-tree CRDT](https://martin.kleppmann.com/2021/10/07/crdt-tree-move-operation.html)
   gives each node a `TreeId` that survives arbitrary concurrent moves (formally
   verified); Unison's content hash is a static-language variant. Identity is
   *recorded, never inferred*.
2. **By operation** — recognize the *edit*.
   [RefactoringMiner](https://users.encs.concordia.ca/~nikolaos/publications/TSE_2020.pdf)
   detects 100+ refactoring types at ~99.9% precision by applying AST
   replacements until statements match (extract/inline resolved via call-site
   context); [CodeShovel](https://www.ncbradley.com/publication/codeshovel/)
   builds method histories through rename/move/signature changes — and tellingly
   *struggles only when a body changes substantially during a move*, the exact
   point where snapshot similarity runs out and only recorded provenance helps.
3. **By snapshot matching** — compare the vector across two anonymous versions
   (GumTree-family). This is the **fallback** for when you failed to capture the
   first two.

The thesis: **record the edit, don't reconstruct it.** Snapshot matching is the
degraded mode; the identity vector above is what you fall back to when identity
was not recorded at write time. For an **agent-authored** codebase this flips
the problem — the agent *is* the editor, so it can emit the operation (rename,
extract, move) as first-class provenance, making identity durable by
construction. The strongest per-line attribution is not better post-hoc
matching; it is capturing edit-intent at the moment of the edit so matching is
never needed.

### Prior art: Unison

[Unison](https://www.unison-lang.org/) is the existence proof that this model
works — it makes **identity = the hash of the normalized AST** a language-level
primitive. Definitions are content-addressed (a Merkle DAG of code, dependencies
referenced by hash; bound variables normalized so alpha-equivalent terms hash
the same), and **names are separate metadata** mapping `name → hash`. The payoff
is exactly the node-identity wishlist, for free: a **rename** is an O(1)
repoint that never touches the hash, and a **move** isn't an event at all, so
attribution pinned to a hash survives both with zero heuristics and zero notes.

Two honest caveats keep this from being a finished answer for git-ast:

- **It doesn't dissolve identity *through an edit*.** Changing a body yields a
  *new* hash — a new entity by construction. Unison records the succession in
  the namespace history (`foo: hash₁ → hash₂`); "the same thing, changed" lives
  in the name binding's history, not in a structural claim. That is a clean
  answer, but the namespace is doing the work, not tree-matching.
- **Unison is greenfield; git-ast is a retrofit.** Unison gets all of this by
  being a new language with a custom content-addressed codebase (not text files,
  not git). git-ast must import the same property into mainstream languages that
  are name- and position-based, stored in git, which is line/blob-addressed.
  Unison never had to solve that retrofit — and the retrofit *is* the open
  problem here.

### Making it possible: a model store + a projection

Unison is the *why*; this is a plausible *how* without inventing a new language.
Split the system into two versioned stores:

- **The model store** holds the **content-addressed AST** — the source of truth,
  where node identity lives durably and is *recorded* rather than recomputed.
- **The projection store** holds the **canonical text** — what humans edit and
  what GitHub, CI, and ordinary `git` see. It is a *derived view* of the model.

They stay in lockstep via the bidirectional transform: a text edit is parsed and
folded back into the model as an identity-preserving mutation; a model change is
re-projected to new canonical text. This is **projectional editing** (cf. MPS,
Hazel) married to dual version control — and git-ast's existing `clean`/`smudge`
round-trip is the seed of that transform. The example dir already anticipates
the split: `04_stored_blob` (the tree) and `05_generated_source` (the
projection) are exactly these two artifacts, promoted to two histories.

**Use [Dolt](https://www.dolthub.com/) for the model store, not a second git.**
The AST is structured data, and the model store's real requirements *are* Dolt's
native features:

- Model the AST as tables (`nodes(id, kind, …)`, `edges(parent, child, field,
  ordinal)`, attribution columns). A node is a **row keyed by stable id** — that
  key *is* its identity.
- Dolt's storage is a prolly tree (a Merkle search tree), so you keep
  content-addressing and structural sharing **and** get efficient three-way
  merge at **cell** granularity. Structural AST merge becomes a native database
  merge instead of an algorithm you write.
- `dolt blame` / `dolt history` operate on a **row**, so **per-node attribution
  is a built-in query** — the per-line-attribution goal, at node granularity, as
  a primitive rather than something reconstructed.

The honest boundaries, so this is an architecture and not a buzzword:

- **Dolt removes the plumbing, not the semantics.** It makes identity cheap to
  store, version, merge, and blame — but *you still choose the keys*, i.e. define
  when two nodes are "the same node" (content hash vs. assigned id). That choice
  is the original hard problem; Dolt does not make it for you.
- **Two heterogeneous stores** (Dolt model + git projection) means a lockstep
  invariant between systems with different merge semantics, and the text→AST
  reconcile heuristic still lives at that boundary — though now it matches an
  edit against a *known prior tree with known ids*, which is far more tractable
  than blind tree-diff.
- **Cell-level conflicts, not zero conflicts.** Two edits to the same node still
  conflict; Dolt just gives a node/cell conflict instead of a line one — strictly
  better, not magic.

### A provenance pipeline (grounding each form of identity)

Tie the pieces into one dataflow, edit → history, and the "record, don't
reconstruct" thesis becomes concrete. Each stage *captures* a form of identity;
the value of the project is moving capture as early (left) as possible, because
everything you fail to capture you must reconstruct heuristically later.

1. **Capture** — the edit's *intent*. An LSP `rename`, an IDE refactor action,
   or an **agent's own edit** is a *typed operation* (rename / extract / move),
   not an anonymous text delta. This is where operation-identity is born; today
   git-ast captures none of it (it sees only the result at `git add`).
2. **Canonicalize** — parse to a deterministic tree and emit canonical bytes.
   This yields *shallow content* identity and a reproducible structure. **git-ast
   does this today.**
3. **Resolve & identify** — run a name resolver (`DefId`-style) and build the
   reference graph; assign stable node ids (content hash à la Unison, or a
   CRDT-style `TreeId`). This populates the rest of the identity vector:
   *binding*, *deep/Merkle content*, *def-vs-use*.
4. **Attribute** — record per-node provenance keyed to the id: author, time, and
   **who/what** produced it (human vs. which agent/model), ideally signed.
5. **Project & preserve** — render canonical text back out (**git-ast does this**)
   and carry identity + attribution through rebase / squash / cherry-pick / merge.

Grounding the identity forms in *what we have vs. what else there is*:

| Identity form | Pipeline stage | Have today | What's needed | Prior art |
|---|---|---|---|---|
| Content (shallow) | 2 Canonicalize | ✅ deterministic canonical bytes | expose subtree hashes | Merkle, Unison |
| Content (deep/Merkle) | 3 Resolve | — | dependency-resolved hash | Unison |
| Name — lexeme | 2 Canonicalize | ✅ present in the text | — | — |
| Name — binding | 3 Resolve | — | a resolver (`DefId`) | Kythe `signature`, LSP |
| Location | 2 Canonicalize | ✅ path / offset | — | git |
| Def vs use/call | 3 Resolve | — | reference graph | [SCIP/LSIF](https://github.com/sourcegraph/scip), Kythe |
| Operation / provenance | 1 Capture | — | editor / agent / LSP op log | RefactoringMiner, [CRDT](https://martin.kleppmann.com/2021/10/07/crdt-tree-move-operation.html), [in-toto](https://in-toto.io/) |
| Authorship (who/what) | 4 Attribute | ✅ commit author at file/line, now reformatting-proof | per-node, human-vs-agent, signed | git blame, [W3C PROV](https://www.w3.org/TR/prov-overview/), [SLSA](https://slsa.dev/)/Sigstore |

Read the table by its columns: **git-ast today owns stages 2 and 5** (the
deterministic canonicalize/project round-trip) and, as a side effect, makes
existing line-level blame survive reformatting. The unbuilt, higher-value work
is stages **1, 3, 4** — capturing the operation, resolving the full identity
vector, and attaching signed per-node authorship. Reframed for an agent-authored
codebase: the agent sits at stage 1, so it can *emit* provenance instead of
leaving it to be recovered — which is exactly the floor reliable per-line
attribution needs.

### Where AST storage hooks in

The natural question is whether the model store is written from the
`clean`/`smudge` filter. Partly:

- **`clean` is the right place to *parse*** — it already does, to canonicalize —
  so emitting the content-addressed AST (subtree hashes, the identity vector) is
  nearly free there and stays pure and deterministic.
- **`clean`/`smudge` are the wrong place to *write* the model store.** A filter
  has no commit context — at `clean` time the blob is not committed, so there is
  no SHA, author, or parent to attribute against or to reconcile the previous
  AST with. Worse, filters also run during `git diff`, `stash`, `archive`, and
  checkout, so a write there would record spurious or read-only states and break
  git's assumption that filters are pure content-in/content-out transforms.

So split responsibilities: **the filter is the codec, commit/ref hooks are the
recorder.** The stateful model-store writes belong in `post-commit` /
`post-merge` (record the new AST + attribution against the parent), `post-rewrite`
(the squash/rebase/amend path — surviving history rewrites), `post-checkout`
(keep model and projection in lockstep), and server-side `post-receive`
(authoritative build). In pipeline terms: `clean` = stage 2, the hooks =
stages 3–4, `smudge` = stage 5.

This also clarifies *what* the model store holds, and why it is not redundant
with the canonical text in git:

- **git** stores the canonical text — the content of record, and (given the
  round-trip) the most compressed encoding of it.
- **the model store (Dolt)** holds the *derived* model, which is two things: a
  **rebuildable index** (AST structure and subtree hashes — recomputable any
  time by reparsing the text, so effectively a cache) and the **non-rebuildable
  provenance** (operation identity, and who/which-agent authored each node) that
  is *not* a function of the text and therefore must be durably stored.

That last distinction is the whole reason a model store exists: git stores what
is reparseable; the model store stores what is not.

### The interface: verbs (verbspec)

The AST surface is naturally a set of **verbs** — operations with a typed input
and output:

- **Read verbs — look at the AST, and at history on the AST.** `inspect` / `find`
  / `refs` (query a snapshot) and `blame` / `log` / `trace` (per-node history).
  The history verbs are the per-line-attribution goal re-expressed on nodes, and
  run over the model store. The query side is achievable first.
- **Write verbs — mutate the AST to generate or refactor.** `rename` / `extract`
  / `inline` / `move` / `generate`. Mutating the tree directly makes each edit a
  *typed operation*, which is stage-1 provenance capture — identity by
  construction, the "record, don't reconstruct" thesis made operational. These
  depend on the resolver (a safe `rename` must update every reference), so they
  sequence after identity.

[**verbspec**](https://github.com/bounded-systems/verbspec) is the delivery
vehicle: a spec-driven framework where you *author a verb once and project it
everywhere* — CLI, MCP, Anthropic tools — from one schema. Authoring the AST
verbs as verbspec verbs is exactly how an **agent** gets AST query / history /
mutation as first-class tools, which is what puts the agent at stage 1 of the
provenance pipeline.

A first read verb ships today: [`git-ast inspect`](./src/printer.rs) lists
top-level definitions with a content hash that is invariant under formatting
(`inspect`, shaped as a verb with `input: { source }`, `output: Def[]`). It is a
proof-of-concept of the read surface — history and write verbs are future work.

## Related projects

- **[frond](https://github.com/bounded-systems/frond)** — the JS/TS counterpart.
  It exercises the same core primitive (parse source to an AST, regenerate it,
  and compare for fidelity) in the JavaScript/TypeScript ecosystem using **SWC**
  on **Deno**, where git-ast uses **Tree-sitter** on Rust. frond focuses on the
  round-trip *fidelity* check — proving a printer can reproduce source faithfully
  — which is exactly the prerequisite git-ast's canonical printer depends on, so
  the two projects validate the same idea across two toolchains.
- **[verbspec](https://github.com/bounded-systems/verbspec)** — a spec-driven CLI
  framework: author a verb once (a typed schema with input/output/run) and
  project it to CLI, MCP, and Anthropic tools. The intended surface for git-ast's
  AST read/write verbs, so the same operations reach humans, agents, and CI from
  one definition. See "The interface: verbs" above.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Contributing

We welcome contributions! Please see our [contribution guidelines](./docs/contributing/guidelines.md) for how to get involved.
