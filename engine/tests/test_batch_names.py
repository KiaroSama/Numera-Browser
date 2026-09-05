"""One name, one profile - and the decision is made where the write happens.

`bulk create` read the taken names, chose a run of free ones around them, and
then asked the repository to write them. Two of those running at once both
passed a check neither repeated under the lock, so both created profiles called
`Bulk-01`. The root lock serialised the writes faithfully; it could not re-make
a decision taken before it was held. From then on `numera Bulk-01` resolved to
one of two profiles and nothing ever said which.

The check belongs inside the transaction that writes, which is `save_many`.
"""

import pytest

from _rendezvous import Rendezvous, finish, start

from numera import generate
from numera.models import Profile
from numera.repo import NameTaken, repo
from numera.store import ProfileStore


def made(name: str) -> Profile:
    return Profile(name=name, fingerprint=generate(seed=1))


def names(root) -> list:
    return sorted(p.name for p in ProfileStore(root).list())


# --- the contract ----------------------------------------------------------

def test_a_batch_cannot_contain_two_of_one_name(tmp_path):
    """Nothing outside the batch is needed for the ambiguity: two pending
    profiles with one name produce it on their own."""
    store = ProfileStore(tmp_path)
    with pytest.raises(NameTaken):
        repo(store).save_many([made("twin"), made("twin")])
    assert names(tmp_path) == [], "a refused batch wrote something"


def test_a_batch_cannot_claim_a_name_the_store_holds(tmp_path):
    store = ProfileStore(tmp_path)
    repo(store).create(made("taken"))
    with pytest.raises(NameTaken):
        repo(store).save_many([made("fresh"), made("taken")])
    assert names(tmp_path) == ["taken"], "a refused batch wrote part of itself"


def test_skip_drops_the_clash_and_keeps_the_rest(tmp_path):
    """`bulk create`'s contract: it is generating a run of names, and it
    already reports how many it passed over."""
    store = ProfileStore(tmp_path)
    repo(store).create(made("taken"))
    saved = repo(store).save_many([made("fresh"), made("taken")],
                                  on_name_clash="skip")
    assert [p.name for p in saved] == ["fresh"]
    assert names(tmp_path) == ["fresh", "taken"]


def test_keeping_the_name_you_already_have_is_not_a_claim(tmp_path):
    """`apply` re-saves every matched profile under its existing name. A store
    that legitimately holds two profiles with one name - `restore --new-id`
    makes exactly that, on purpose - must not make an unrelated bulk edit fail
    on a duplicate it did not create."""
    store = ProfileStore(tmp_path)
    first = repo(store).create(made("same"))
    second = repo(store).create(made("same"), unique_name=False)
    for prof in (first, second):
        prof.notes = "bulk"

    repo(store).save_many([first, second])          # the rejecting policy
    assert [p.notes for p in store.list()] == ["bulk", "bulk"]


def test_renaming_onto_a_taken_name_is_refused(tmp_path):
    store = ProfileStore(tmp_path)
    repo(store).create(made("a"))
    b = repo(store).create(made("b"))
    b.name = "a"
    with pytest.raises(NameTaken):
        repo(store).save_many([b])
    assert names(tmp_path) == ["a", "b"]


def test_an_unnamed_profile_clashes_with_nothing(tmp_path):
    store = ProfileStore(tmp_path)
    saved = repo(store).save_many([made(""), made("")])
    assert len(saved) == 2


def test_an_unknown_policy_is_refused(tmp_path):
    with pytest.raises(ValueError):
        repo(ProfileStore(tmp_path)).save_many([], on_name_clash="whatever")


# --- and two real processes cannot talk each other into a duplicate --------

BULK = """
from numera import cli
wait()
raise SystemExit(cli.main(["--data-dir", ROOT, "bulk-create", "--count", "6",
                           "--prefix", "Bulk", "--json"]))
"""


def test_two_processes_bulk_creating_one_prefix_make_no_duplicates(tmp_path):
    """Both are poised on the same names when they are released together.

    The uniqueness contract is what is under test, not which of them wins: the
    prefix and index make the names deterministic, so the second process is
    asking for exactly the names the first is creating, and skipping them is
    the answer. What must never happen is twelve profiles under six names.
    """
    bar = Rendezvous()
    procs = {who: start(BULK, tmp_path, who, bar.port) for who in ("a", "b")}
    try:
        for who in procs:
            bar.release(who)             # both are already waiting to be let go
        results = finish(procs)
    finally:
        bar.close()
        for proc in procs.values():
            if proc.poll() is None:
                proc.kill()

    for who, result in results.items():
        assert result["code"] == 0, f"{who} failed: {result}"
    made_names = names(tmp_path)
    assert len(made_names) == len(set(made_names)), \
        f"two profiles share a name: {made_names}"
    assert len(made_names) == 6, f"expected the six requested names, got {made_names}"
