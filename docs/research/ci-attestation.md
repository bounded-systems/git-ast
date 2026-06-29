# Content-addressed CI attestation: "has CI run against this projection?"

> A design note, companion to [`node-identity.md`](./node-identity.md) and
> [`storage-model.md`](./storage-model.md). It applies git-ast's **formatting-
> invariant projection hash** as the key for an attestation — *"CI ran against this
> exact content, here's the signed verdict"* — so "has this been verified?" becomes a
> **content-addressed lookup** you can show, not a claim you take on faith.

## The idea

Key a CI/static-analysis result by the **content hash of the canonical projection**
(what git-ast's clean filter already produces), not by the file's git blob SHA. Then:

- given any code, compute its projection hash and **look up** whether CI ran against
  that exact structure — `✅ verified (run #…, signed)` or `⚠️ not analyzed`;
- the result is **robust to reformatting and reordering**, because the projection hash
  is formatting- and order-invariant (a git blob SHA is not).

This is the bounded-systems posture applied to verification: not *"trust that CI ran"*
— **here is the content hash and the signed verdict; verify it yourself.**

## Why the *projection*, specifically

A naive content-hash CI cache keyed on the raw file busts on every whitespace change.
git-ast's projection hash is a pure function of the parse tree (the "Determinism
contract" in [`src/printer.rs`](../../src/printer.rs)), so:

- a pure reformat → **same key** → the attestation still applies;
- reordering top-level items → **same key** (matching is position-independent);
- a real structural change → **different key** → correctly forces re-analysis.

You are attesting against **structural identity**, which is exactly what a
content-addressed store of *raw* files cannot give you and git-ast can.

## Mechanism (mostly composition of parts that already exist)

```
source ──(git-ast clean filter)──▶ canonical projection ──▶ projection_hash
                                                                  │
   CI / static analysis runs ─────────────────────────────────▶  │
                                                                  ▼
        attestation { projection_hash, checks[], verdict,
                      ci_run_id, toolchain, timestamp }  ──signed──▶ anchored-chain
                                                                  │
                                            stored in CAS, keyed by projection_hash
                                                                  │
                       "show it":  git ast verified <file> | synoptic coverage column
```

- **CAS** already stores content-addressed blobs (e.g. the Slack JSONL dumps) — here
  it holds `projection_hash → attestation`.
- **anchored-chain** already does *Ed25519 over canonical manifests → in-toto / DSSE*
  (a ✅ trust-ledger row). A CI-result attestation is just a **new claim type** in
  machinery already shipped — the "manifest" is `{projection_hash, checks, verdict}`.
- **synoptic** (the fleet board) is the natural surface to **show coverage**: which
  projections across the corpus carry a fresh CI attestation, which are stale/absent.
- **node identity** (this repo's `match`/`inspect`) gives **granularity**: per-item
  `content_hash` → attest CI *per function*, so a PR only re-verifies the nodes it
  actually changed. Incremental CI keyed by node identity — the node-identity arc
  applied to verification.

## Three views of one mechanism

1. **Attestation / "show it"** (primary; trust-ledger flavored). A signed, content-
   addressed proof that a given structure passed a named set of checks. Answers
   *"is this verified, and can I check that independently?"*
2. **Incremental / memoized CI** (build-speed flavored). Skip re-running analysis on a
   projection hash already attested — and, at node granularity, re-check only changed
   nodes. Robust to reformat (unlike blob-SHA caches).
3. **Corpus coverage** (synoptic flavored). A map over the org corpus of *analyzed vs
   not*, keyed by structural identity rather than commit — survives reformatting churn.

## The soundness rule (this decides the design)

**The hash must cover everything the check depends on, or you attest a stale green.**

- **Syntactic checks** (format, lint, simple AST rules) depend only on the node/file →
  the **shallow** projection hash is a sound key. Implementable today.
- **Semantic checks** (type-check, tests, anything cross-module) also depend on
  *dependencies* → they require the **deep / transitive Merkle hash** (Unison's "deep
  content"; the shallow-vs-deep distinction from
  [`node-identity.md`](./node-identity.md)). Key a type-check by the shallow hash and
  you will serve a `pass` after a dependency changed underneath it — not a cache bug
  but a **false attestation**, which is fatal for a verify-don't-trust ledger.

**Design consequence:** start with **syntactic/lint attestation on the shallow hash**
(sound now), and *gate* semantic-check attestation on having the deep transitive hash
— which is itself a node-identity borrow already on the roadmap (`prx-zoc7`).

## Recommendation / next steps

1. Define the attestation schema `{ projection_hash, checks[], verdict, ci_run_id,
   toolchain_id, timestamp }` and sign it via anchored-chain (reuse the existing
   canonical-manifest signer).
2. Ship the **syntactic** slice first (lint/format keyed on the shallow projection
   hash) — sound, and a clean trust-ledger row ("CI-verified, content-addressed").
3. Add a **show** surface: a `git ast verified <file>` lookup and/or a synoptic
   coverage column.
4. Defer **semantic** attestation until the deep transitive hash exists; until then,
   mark semantic verdicts explicitly as *shallow-keyed (not dependency-sound)* rather
   than over-claiming.

## Open questions

- What is the canonical **toolchain identity** in the key? A lint verdict is only valid
  for a specific linter+config+version; the attestation must bind the toolchain, or a
  config change silently inherits a stale green. (Same shape as git-ast's own
  `(grammar, printer)` versioning of the canonical form.)
- **Granularity vs. cross-cutting checks:** per-node attestation is clean for
  node-local checks, but whole-file/whole-crate checks (a module-level lint) don't
  decompose — they key on the file/crate projection hash, not a node hash.
- Where does the attestation **store** live relative to [`storage-model.md`](./storage-model.md)'s
  Dolt proposal — is the CI-attestation table just another table in the same versioned
  store?
