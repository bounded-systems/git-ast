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

**Working clean/smudge round-trip for a Rust subset.** The core pipeline is
implemented and runs through real Git:

- `git-ast setup` registers the filter in a repository.
- On `git add`, the `clean` filter parses Rust with Tree-sitter and stores its
  **canonical** form; on `git checkout`, `smudge` returns it. Reformatting
  therefore never reaches history — two differently-formatted inputs that parse
  to the same tree produce byte-identical blobs.
- It speaks Git's real `filter-process` pkt-line protocol, so `git add` /
  `git checkout` / `git diff` all work end to end. See
  [`examples/demo.sh`](./examples/demo.sh).

Honest boundaries:

- **One language, a subset of it.** The pretty-printer covers the constructs in
  the example (functions, params, blocks, `let`, binary/call/macro expressions,
  literals, comments). It is **fail-closed**: syntax errors reject the commit,
  and any unsupported construct returns an error rather than corrupting code.
  Widening coverage is additive — one more arm per node kind.
- **Diff and merge drivers are still placeholders.** Making those *structural*
  depends on the hardest open problem — **stable AST node identity across
  versions** — which this does **not** solve. Canonical formatting removes
  formatting churn from history; it does not yet track a node through a move or
  rename. That problem is described in
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

## Related projects

- **[frond](https://github.com/bounded-systems/frond)** — the JS/TS counterpart.
  It exercises the same core primitive (parse source to an AST, regenerate it,
  and compare for fidelity) in the JavaScript/TypeScript ecosystem using **SWC**
  on **Deno**, where git-ast uses **Tree-sitter** on Rust. frond focuses on the
  round-trip *fidelity* check — proving a printer can reproduce source faithfully
  — which is exactly the prerequisite git-ast's canonical printer depends on, so
  the two projects validate the same idea across two toolchains.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Contributing

We welcome contributions! Please see our [contribution guidelines](./docs/contributing/guidelines.md) for how to get involved.
