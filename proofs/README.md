# Lean proofs

Machine-checked soundness for git-ast's structural 3-way JSON merge — the *formal*
half of "backed by Rust **and** Lean".

- [`JsonMerge.lean`](./JsonMerge.lean) formalizes the algorithm in
  [`../src/merge.rs`](../src/merge.rs) (`merge3`) and **proves** its core soundness
  properties:
  - `merge3_idem` — merging a value with itself yields that value (no spurious change).
  - `merge3_onlyOurs` / `merge3_onlyTheirs` — if exactly one side changed, the merge takes that side.
  These hold **universally** (for all values), and are `#print axioms`-clean
  (`[propext]` only — no `sorry`).
- The same file **`decide`s** the conformance vectors from
  [`../tests/merge_vectors.json`](../tests/merge_vectors.json) — the *shared spec*
  that the Rust test suite also runs. Lean decides the exact cases Rust executes,
  so the implementation and the proof agree on a common ground truth.

## Scope (stated honestly)

The Lean model is **one level** (scalar values, flat objects): a both-sides change
to the same key is a conflict here. The Rust implementation recurses into nested
objects; that recursive case is conformance-tested on the Rust side (the
`nested_different_subkeys` vector). The soundness theorems are nonetheless fully
general — they are decided before any object-level merge. Two levels also keeps
`Json` non-recursive, so `DecidableEq` derives in core Lean (it does not for a
`List`-nested type) — this is why the model is split into `Scalar` / `Json`.

Mathlib-free (Lean 4 core only) — the whole proof checks in well under a second.

## Checking it

```sh
# with elan (reads ./lean-toolchain → leanprover/lean4:v4.31.0)
lake build                       # or: lake env lean JsonMerge.lean
```

CI runs this on every PR (the `proofs` job in `.github/workflows/ci.yml`) and
fails if any proof depends on `sorry`.

## Roadmap

- Symmetry (`merge3 o a b ≅ merge3 o b a`) and the deep "non-conflicting edits are
  preserved" theorem need canonical key ordering (which the clean filter provides,
  claim 8.1a) — a follow-up.
- A recursive (nested-object) model to decide the `nested_*` vectors in Lean too.
