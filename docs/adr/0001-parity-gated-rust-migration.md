# Rust migration boundary

Status: superseded in November 2026 by the owner's complete Rust rewrite request.

The original October 7 decision retained the Python application during migration
and proposed a React/TypeScript UI. The owner subsequently selected Rust for the
application, dashboard, browser preparation, tests and build tooling, authorized
retiring the previous implementation, and required one unmerged pull request.

The current architecture is documented in [architecture](../architecture.md) and
[rewrite verification](../rust-rewrite.md). Rust source owns application behavior;
compiler-generated browser bindings and independently licensed vendor browser/SDK
resources remain explicit distribution artifacts. Required behavioral, security,
process-ownership and platform checks still apply to the current source revision.
