# Research notes

Design and literature notes behind git-ast's node-identity work (trust ledger row
8.1d). They build on one another:

- **[identity-index.md](./identity-index.md)** — *the keystone.* One content-addressed
  **Merkle trie/DAG of canonical subtrees**, keyed by a chosen equivalence hash, is the
  substrate under the other three: compare / persist / verify are its three faces.
- **[node-identity.md](./node-identity.md)** — *compare.* A verified survey of the
  **construct / compute / resolve** strategies (Unison, GumTree, Kythe/SCIP/stack-graphs)
  and what git-ast should borrow; establishes why the honest grade is 🟡.
- **[storage-model.md](./storage-model.md)** — *persist.* A versioned structured store
  (Dolt; prolly trees *are* Merkle tries) as the identity persistence + anchoring +
  cross-file index layer — the middle path between compute and construct.
- **[ci-attestation.md](./ci-attestation.md)** — *verify.* Key CI/static-analysis
  verdicts by the formatting-invariant projection hash, so *"has CI run against this
  structure?"* is a content-addressed, showable, signed lookup.
