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

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Contributing

We welcome contributions! Please see our [contribution guidelines](./docs/contributing/guidelines.md) for how to get involved.
