/-
  Machine-checked soundness of git-ast's structural 3-way JSON merge.

  This formalizes the algorithm in `src/merge.rs::merge3` and proves its core
  soundness properties. It is the formal half of "backed by Rust *and* Lean":
  the Rust side *executes* the merge (and is conformance-tested on the vectors in
  `tests/merge_vectors.json`); this Lean side *proves* the properties universally
  and *decides* the same shared vectors, so the two agree on a common spec.

  Dependencies: Lean 4 core only (no Mathlib) — keeps the proof build small.

  Modeling notes (scope, stated honestly):
  * Numbers are integers. Number representation is orthogonal to the merge logic.
  * Objects are association lists; *canonical key ordering* is the clean filter's
    job (trust claim 8.1a, `src/json.rs`), not the merge's, so this model keeps
    first-occurrence order.
  * The model is **one level**: object values are scalars, so a both-sides change
    to the same key is a conflict here. The Rust implementation recurses into
    nested objects; that recursive case is conformance-tested on the Rust side
    (the `nested_different_subkeys` vector). The soundness theorems below are
    nonetheless fully general — they are decided before any object-level merge.
  * Two levels (scalar / flat object) also keeps `Json` non-recursive, so
    `DecidableEq` derives in core Lean (it does not, for a `List`-nested type).
-/

namespace GitAst

/-- A scalar JSON value. -/
inductive Scalar where
  | null
  | bool (b : Bool)
  | num (n : Int)
  | str (s : String)
deriving DecidableEq, Repr

/-- A JSON value: a scalar, or a flat object of scalars (see modeling notes). -/
inductive Json where
  | scalar (s : Scalar)
  | obj (kvs : List (String × Scalar))
deriving DecidableEq, Repr

/-- Result of a 3-way merge. -/
inductive Merge where
  | clean (v : Json)
  | conflict
deriving DecidableEq, Repr

namespace Json

/-- Association-list lookup by key. -/
def find : List (String × Scalar) → String → Option Scalar
  | [], _ => none
  | (k, v) :: t, key => if key = k then some v else find t key

/-- Keys of an object, in order. -/
def keysOf : List (String × Scalar) → List String
  | [] => []
  | (k, _) :: t => k :: keysOf t

/-- Deduplicate keys, preserving first-occurrence order. -/
def dedupAux : List String → List String → List String
  | _, [] => []
  | seen, k :: t => if seen.contains k then dedupAux seen t else k :: dedupAux (k :: seen) t

def dedup (l : List String) : List String := dedupAux [] l

/-- One-key merge. `o a b` are the base / ours / theirs values for a key (absent
    = `none`). Result `none` = key absent in the merge; `some (clean v)` = `v`. -/
def mergeOptFlat (o a b : Option Scalar) : Option Merge :=
  if a = b then a.map fun s => Merge.clean (Json.scalar s)        -- same on both sides
  else if o = a then b.map fun s => Merge.clean (Json.scalar s)   -- only theirs changed
  else if o = b then a.map fun s => Merge.clean (Json.scalar s)   -- only ours changed
  else some Merge.conflict                                         -- divergent

/-- Merge a list of keys across three objects into one result object. -/
def mergeKeys (ob aa bb : List (String × Scalar)) : List String → Merge
  | [] => Merge.clean (Json.obj [])
  | k :: ks =>
    match mergeOptFlat (find ob k) (find aa k) (find bb k), mergeKeys ob aa bb ks with
    | _, Merge.conflict => Merge.conflict
    | none, rest => rest
    | some Merge.conflict, _ => Merge.conflict
    | some (Merge.clean (Json.scalar v)), Merge.clean (Json.obj kvs) =>
        Merge.clean (Json.obj ((k, v) :: kvs))
    | some (Merge.clean _), Merge.clean other => Merge.clean other  -- unreachable

/-- The structural 3-way merge, mirroring `src/merge.rs::merge3`. -/
def merge3 (o a b : Json) : Merge :=
  if a = b then Merge.clean a              -- same change (or no change)
  else if o = a then Merge.clean b         -- only theirs changed
  else if o = b then Merge.clean a         -- only ours changed
  else match o, a, b with
    | Json.obj ob, Json.obj aa, Json.obj bb =>
        mergeKeys ob aa bb (dedup (keysOf ob ++ keysOf aa ++ keysOf bb))
    | _, _, _ => Merge.conflict

/-! ## Soundness theorems (universal — hold for all values). -/

/-- No spurious change: merging a value with itself yields that value. -/
theorem merge3_idem (j : Json) : merge3 j j j = Merge.clean j := by
  simp [merge3]

/-- Only ours changed: if theirs equals base, the merge takes ours. -/
theorem merge3_onlyOurs (o a : Json) : merge3 o a o = Merge.clean a := by
  unfold merge3
  by_cases h1 : a = o
  · rw [if_pos h1]
  · rw [if_neg h1]
    by_cases h2 : o = a
    · exact absurd h2.symm h1
    · rw [if_neg h2, if_pos rfl]

/-- Only theirs changed: if ours equals base, the merge takes theirs. -/
theorem merge3_onlyTheirs (o b : Json) : merge3 o o b = Merge.clean b := by
  unfold merge3
  by_cases h1 : o = b
  · subst h1; rw [if_pos rfl]
  · rw [if_neg h1, if_pos rfl]

/-! ## Conformance: decide the shared vectors from `tests/merge_vectors.json`.

    Lean *decides* the exact cases the Rust test suite *executes*. The nested
    vector (`nested_different_subkeys`) exercises recursion and is checked on the
    Rust side only (see the one-level modeling note above). -/

private def n (i : Int) : Scalar := Scalar.num i
private def o1 (k : String) (v : Int) : Json := Json.obj [(k, n v)]
private def o2 (k1 : String) (v1 : Int) (k2 : String) (v2 : Int) : Json :=
  Json.obj [(k1, n v1), (k2, n v2)]

-- 1. no_change
example : merge3 (o1 "a" 1) (o1 "a" 1) (o1 "a" 1) = Merge.clean (o1 "a" 1) := by decide
-- 2. only_ours
example : merge3 (o1 "a" 1) (o1 "a" 2) (o1 "a" 1) = Merge.clean (o1 "a" 2) := by decide
-- 3. only_theirs
example : merge3 (o1 "a" 1) (o1 "a" 1) (o1 "a" 9) = Merge.clean (o1 "a" 9) := by decide
-- 4. different_keys
example :
    merge3 (o2 "a" 1 "b" 1) (o2 "a" 2 "b" 1) (o2 "a" 1 "b" 3)
      = Merge.clean (o2 "a" 2 "b" 3) := by decide
-- 5. same_key_conflict
example : merge3 (o1 "a" 1) (o1 "a" 2) (o1 "a" 3) = Merge.conflict := by decide
-- 6. add_distinct_keys
example :
    merge3 (Json.obj []) (o1 "a" 1) (o1 "b" 2) = Merge.clean (o2 "a" 1 "b" 2) := by decide

/-! ## sorry-free guard.

    `#print axioms` lists the axioms each proof depends on. A `sorry` would appear
    as `sorryAx`; these print only the standard logical axioms, so the proofs are
    complete. -/
#print axioms merge3_idem
#print axioms merge3_onlyOurs
#print axioms merge3_onlyTheirs

end Json
end GitAst
