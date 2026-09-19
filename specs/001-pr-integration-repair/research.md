# Phase 0 — Research

One question blocked the plan. It was settled by measurement on the machine where the failure
appears, not by reading the diff.

## Q: is the surviving grandchild a production defect, or an artefact of the assertion?

**Context.** `test_failed_report_after_a_descendant_exists_ends_the_whole_tree` (new in #20)
fails on native Windows. It forces the ownership report to fail after a real grandchild
exists, then asserts the whole tree ended. The leader ends; `proctree.alive(pid, token)` for
the grandchild answers `True`.

The brief claims this path is repaired, and its own evidence section admits native Windows was
never executed. Both readings were open:

- the production path ends only the leader on Windows, and the test is right to fail; or
- the production path ends the tree correctly, and the test asks too early.

They are distinguishable because the production code says which one happened. On the failure
path, `disposal.reported` calls `proctree.end_tree`; if that returns `False` it raises an
*enriched* error containing "may still be running". If it returns `True` it re-raises the
original. The test asserts only the exception type, so it never reads that difference.

**Method.** A standalone reproduction of the same scenario, printing the facts the test hides:
which error was raised, whether `alive_tree` is false, and whether the grandchild is alive —
sampled immediately and then polled.

**Result** (three runs, consistent):

| Observation | Value |
|---|---|
| error raised | the original, `fixture record failure` — so `end_tree` returned `True` |
| `alive_tree(tree)` immediately after | `False` |
| `alive(grandchild)` immediately after | `True` |
| `alive(grandchild)` after polling | `False` after **10, 10 and 11 ms** |

**Decision.** The production behaviour is correct. `end_tree` terminated the job, the job
proved empty, and the descendant did die. What fails is the assertion: on Windows a terminated
process keeps answering a liveness question for a short interval after its job reports empty,
and the test samples once, immediately.

**Rationale.** Two independent facts support it over the alternative. The job-level proof and
the per-pid probe disagree only during that interval and agree after it, which is the
signature of a teardown race rather than a missed kill. And the product's own tree proof,
`prove_empty`, already polls to a deadline instead of sampling once — the test is the only
place in this path that asks the question in a single shot.

**Alternatives considered.**

- *Change the production code to wait before returning.* Rejected: it would make every
  successful stop slower to satisfy a test, and `end_tree` already proves what it claims —
  that the tree is empty. The stronger per-process claim belongs to the caller that wants it.
- *Assert on `alive_tree` instead of the grandchild.* Rejected: it would pass, and it would
  stop testing the scenario's actual subject. The whole point of this case is that a
  *descendant* is reached, not merely the leader's job handle.
- *Raise the assertion's tolerance by sleeping a fixed amount.* Rejected by the project's own
  test rules: a blind sleep is not a readiness signal, and 10 ms measured on an idle
  workstation is not a bound to hard-code on a loaded CI runner.

**Consequence for the plan.** The repair is one bounded poll in the test's assertion helper,
on #20's branch. No production file changes.
