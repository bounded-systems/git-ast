# Project Overview

> **Status:** design stage. See the repository [README](../README.md#project-status)
> for what is and isn't implemented.

## Goal

Make Git **language-aware**: have it understand code as structure (an Abstract or
Concrete Syntax Tree) rather than as lines of text, so that history, diffs, and
merges reflect *semantic* change instead of textual change.

## Mechanism

Git AST plugs into Git's existing extension points rather than replacing Git:

1. **Clean filter (`git add`):** parse source into a syntax tree (via Tree-sitter)
   and store the serialized tree as the Git blob.
2. **Smudge filter (`git checkout`):** deserialize the tree and pretty-print it
   back to canonical source.
3. **Diff driver:** compare trees structurally so formatting-only changes
   disappear from diffs.
4. **Merge driver:** perform 3-way merges on tree structure to auto-resolve
   conflicts caused by code movement or non-overlapping edits.

Because the work happens inside Git's filter/driver system, developers keep using
ordinary `git` commands and ordinary source files in their working tree.

## Why it matters

- **Cleaner diffs** — review focuses on meaningful change, not whitespace churn.
- **Smarter merges** — fewer conflicts from reformatting and code movement.
- **Consistent formatting** — checkout always produces canonical style.

## Key tension

Tree-aware history depends on **stable node identity** — being able to say "this
function is the same function, moved" across commits. That is hard, and it is the
crux of refactor-aware features such as semantic blame. The MVP deliberately
defers it; see [`planning/scope.md`](./planning/scope.md) and
[`future-directions.md`](./future-directions.md).

## Where to go next

- [Key Concepts](./concepts/key-concepts.md)
- [Architecture Design](./architecture/design.md)
- [Roadmap](./roadmap.md)
