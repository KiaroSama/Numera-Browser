# Isolated Rust secret-scan bootstrap

The Gitleaks workflow verifies the manifest, lockfile and two source-file SHA-256
pins before copying those four files outside the checkout and compiling them with
Rust 1.99.1. It does not execute application crates, repository Cargo configuration
or repository build scripts. Updates to this bootstrap require reviewing and
updating all four workflow pins.

The bootstrap verifies Gitleaks 8.30.1, the official rules and the reviewed project
configuration. Public-data exceptions have exact paths and values. The synthetic
Windows signing fixture has exact byte, size and provenance pins; extra signing
material is rejected. Before history scanning, a bounded NUL-framed Git log
enumerates every historical addition, modification and deletion of signing
material. Actual blob packets must match the reviewed pins, even when later
commits restore or delete the file. Unknown paths, aliases, link modes, shallow
history and malformed records fail the scan. Repository ignore files and inline ignore comments are
disabled. Built-in broad path exclusions are removed before the pinned rules run.

A private, hash-bound copy of the same executable records completion of the actual
Git history producer. Missing, malformed or failed Git receipts fail the scan even
when Gitleaks returns zero. Only valid reports agreeing with the scanner exit code
can pass. Timeouts, bounded private output and cleanup apply to each run. Summaries
contain only escaped rule, path, line and commit identifiers, never matched values,
source snippets or commit messages.

`source_ci` exercises these pure contracts under its existing secret-review case.
The live Git-producer failure control uses the real scanner with a Rust Git fixture
that exits 7 for history reads. Full-history and PR-range scans are separate scopes;
a clean result means no matching findings under these rules, not universal secret
or byte coverage. There is no automatic upload of the private scanner report.
