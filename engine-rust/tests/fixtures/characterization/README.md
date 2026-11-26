# Frozen native characterization observations

These documents replace the live interpreter portion of the existing native
parity tests. They were produced by executing the original observers against
immutable Git source, using CPython 3.13.15 and Unicode 15.1.0. The native test
names, case loops, count assertions, input checks and behavior assertions remain.
The observations are test data; the product and test runner do not execute Python.

`manifest.json` records the source commit/tree, observer digest, output digest
and observation counts for each test. The shared identity regression verifies
all listed payload hashes. Most observations retain historical commit `13b6d9c`.
The complete CLI registry uses the selected checkpoint `3bf1d839`, which added
exact-offer Cloak consent flags and their parser defaults. Its provenance states
that source update explicitly. The separately mapped source suites cover every
original application case at that checkpoint.

All credentials, profile identities and browser observations are synthetic test
inputs. These fixtures do not replace real process, browser or filesystem tests.
