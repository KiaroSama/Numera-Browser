"""Bounded numeric-only process snapshots, never benchmark acceptance metrics."""
from __future__ import annotations

from itertools import islice
from pathlib import Path
import re
import sys
import time

from numera import proctree, unit_report

MAX_PROCESSES = 512
MAX_SNAPSHOTS = 1800
MAX_WINDOWS_INTERVALS = 128
ROLES = ('command', 'sdk-leader', 'owned-descendant')
INTERVAL = .05


def read(path):
    with path.open('rb') as stream:
        raw = stream.read(65537)
    if len(raw) > 65536:
        raise ValueError('Memory diagnostic input exceeds its bound')
    return raw


def identity(raw):
    end = raw.rindex(b')')
    pid = int(raw.split(b' ', 1)[0])
    fields = raw[end + 2:].split()
    group, birth = int(fields[2]), int(fields[19])
    if min(pid, group, birth) <= 0:
        raise ValueError('Memory diagnostic process identity is invalid')
    return pid, group, birth


def linux_snapshot(groups, *, root=Path('/proc')):
    rows, missing, visited = [], 0, 0

    def admitted():
        valid = set()
        for group, token in groups.items():
            try:
                if identity(read(root / str(group) / 'stat')) == (group, group, token):
                    valid.add(group)
            except (OSError, ValueError, IndexError):
                pass
        return valid

    active = admitted()
    missing += len(groups) - len(active)
    # Membership comes from birth-checked owned groups, never command lines.
    for entry in islice(root.iterdir(), 8193):
        visited += 1
        if visited > 8192:
            missing += 1
            break
        if not entry.name.isascii() or not entry.name.isdecimal():
            continue
        try:
            before = identity(read(entry / 'stat'))
            if before[0] != int(entry.name):
                raise ValueError('Memory diagnostic PID differs')
            if before[1] not in active:
                continue
            if len(rows) >= MAX_PROCESSES:
                missing += 1
                break
            counters = {}
            for line in read(entry / 'status').splitlines():
                if line.startswith((b'VmRSS:', b'VmHWM:')):
                    match = re.fullmatch(rb'(VmRSS|VmHWM):\s+([0-9]{1,16}) kB', line)
                    if not match or match[1] in counters:
                        raise ValueError('Memory diagnostic counter is invalid')
                    counters[match[1]] = int(match[2]) * 1024
            if set(counters) != {b'VmRSS', b'VmHWM'}:
                raise ValueError('Memory diagnostic counters are incomplete')
            if identity(read(entry / 'stat')) != before:
                raise ValueError('Memory diagnostic identity changed during observation')
            rows.append((before[0], before[2], {'rss_bytes': counters[b'VmRSS'],
                                               'peak_rss_bytes': counters[b'VmHWM']}))
        except (OSError, ValueError, IndexError):
            missing += 1
    still_owned = admitted()
    if still_owned != active:
        missing += len(active - still_owned)
        # A changed group leader invalidates this whole non-atomic snapshot.
        rows = []
    return sorted(rows), missing


def log_evidence(value, runtime, logger):
    keys = {'coverage', 'snapshots', 'missing', 'truncated', 'roles'}
    if (runtime not in ('original', 'native') or not isinstance(value, dict)
            or set(value) not in (keys, keys | {'windows_intervals'})
            or value['coverage'] != 'sampled-not-lifetime-attribution'
            or type(value['truncated']) is not bool
            or any(type(value[key]) is not int or not 0 <= value[key] <= 2**64 - 1
                   for key in ('snapshots', 'missing'))
            or not isinstance(value['roles'], dict)):
        raise ValueError('Memory diagnostic evidence is invalid')
    roles = value['roles']
    if not set(roles) <= {'command', 'sdk-leader', 'owned-descendant'}:
        raise ValueError('Memory diagnostic role is invalid')
    for row in roles.values():
        if (not isinstance(row, dict) or set(row) != {'identities', 'max_observed'}
                or type(row['identities']) is not int or not 1 <= row['identities'] <= MAX_PROCESSES
                or not isinstance(row['max_observed'], dict)
                or not set(row['max_observed']) <= {
                    'rss_bytes', 'peak_rss_bytes', 'private_bytes', 'peak_private_bytes'}
                or any(type(number) is not int or not 0 <= number <= 2**64 - 1
                       for number in row['max_observed'].values())):
            raise ValueError('Memory diagnostic counters are invalid')
    intervals = value.get('windows_intervals', [])
    fields = {'started_ns', 'finished_ns', 'peak_before_bytes', 'peak_after_bytes',
              'membership_changed', 'missing', 'private_bytes', 'members'}
    if not isinstance(intervals, list) or len(intervals) > MAX_WINDOWS_INTERVALS:
        raise ValueError('Memory diagnostic intervals exceed their bound')
    previous_end = previous_peak = 0
    for interval in intervals:
        if (not isinstance(interval, dict) or set(interval) != fields
                or type(interval['membership_changed']) is not bool
                or any(type(interval[key]) is not int or not 0 <= interval[key] <= 2**64 - 1
                       for key in fields - {'membership_changed', 'private_bytes', 'members'})
                or not previous_end <= interval['started_ns'] <= interval['finished_ns']
                or not previous_peak <= interval['peak_before_bytes'] <= interval['peak_after_bytes']
                or any(not isinstance(interval[key], dict) or set(interval[key]) != set(ROLES)
                       or any(type(number) is not int or not 0 <= number <= 2**64 - 1
                              for number in interval[key].values())
                       for key in ('private_bytes', 'members'))
                or sum(interval['members'].values()) > MAX_PROCESSES):
            raise ValueError('Memory diagnostic interval is invalid')
        previous_end, previous_peak = interval['finished_ns'], interval['peak_after_bytes']
    logger.warning('Unmeasured memory coverage=sampled-not-lifetime-attribution runtime=%s '
                   'snapshots=%s missing=%s truncated=%s',
                   runtime, value['snapshots'], value['missing'], value['truncated'])
    for role, row in roles.items():
        for metric, number in row['max_observed'].items():
            logger.warning('Unmeasured memory runtime=%s role=%s identities=%s metric=%s max_observed=%s',
                           runtime, role, row['identities'], metric, number)
    for index, interval in enumerate(intervals):
        logger.warning(
            'Unmeasured memory coverage=non-atomic-current-private runtime=%s interval=%s '
            'started_ns=%s finished_ns=%s peak_before_bytes=%s peak_after_bytes=%s '
            'membership_changed=%s missing=%s command_private_bytes=%s sdk_leader_private_bytes=%s '
            'owned_descendant_private_bytes=%s command_members=%s sdk_leader_members=%s '
            'owned_descendant_members=%s', runtime, index,
            *(interval[key] for key in ('started_ns', 'finished_ns', 'peak_before_bytes',
                                       'peak_after_bytes', 'membership_changed', 'missing')),
            *(interval['private_bytes'][role] for role in ROLES),
            *(interval['members'][role] for role in ROLES))


class Recorder:
    def __init__(self, tree, report, prior, job):
        self.tree, self.report, self.prior, self.job = tree, report, prior, job
        self.command_token = tree.token if tree.token is not None else proctree.started_at(tree.pid)
        self.next = 0.0
        self.snapshots = self.missing = 0
        self.truncated = False
        self.roles = {}
        self.identities = {}
        self.windows_intervals = [] if sys.platform == 'win32' else None

    def sample(self):
        now = time.monotonic()
        if now < self.next:
            return
        self.next = now + INTERVAL
        if self.snapshots >= MAX_SNAPSHOTS:
            self.truncated = True
            return
        self.snapshots += 1
        receipt = unit_report.read(self.report)
        if not receipt.complete:
            self.missing += 1
            return
        trees = [tree for tree in receipt.trees if tree not in self.prior and tree != self.tree]
        leaders = {(tree.pid, tree.token) for tree in trees}
        try:
            interval = None
            if sys.platform == 'linux':
                groups = {tree.group: tree.token for tree in (self.tree, *trees) if tree.kind == 'pgid'}
                rows, missing = linux_snapshot(groups)
            elif sys.platform == 'win32' and self.job is not None:
                from benchmark_memory_windows import joint_snapshot
                rows, missing, interval = joint_snapshot(self.job)
                interval.update(missing=missing, private_bytes=dict.fromkeys(ROLES, 0),
                                members=dict.fromkeys(ROLES, 0))
            else:
                raise ValueError('Unsupported memory diagnostic platform')
            self.missing += missing
            for pid, birth, counters in rows:
                role = ('command' if (pid, birth) == (self.tree.pid, self.command_token) else
                        'sdk-leader' if (pid, birth) in leaders else 'owned-descendant')
                known = self.identities.setdefault(role, set())
                if (pid, birth) not in known and sum(map(len, self.identities.values())) >= MAX_PROCESSES:
                    self.truncated = True
                    if interval is not None:
                        interval['missing'] += 1
                    continue
                known.add((pid, birth))
                maximum = self.roles.setdefault(role, {})
                for key, value in counters.items():
                    maximum[key] = max(maximum.get(key, 0), value)
                if interval is not None:
                    interval['private_bytes'][role] += counters['private_bytes']
                    interval['members'][role] += 1
            if interval is not None:
                if len(self.windows_intervals) < MAX_WINDOWS_INTERVALS:
                    self.windows_intervals.append(interval)
                else:
                    self.truncated = True
        except (OSError, ValueError, IndexError):
            self.missing += 1

    def result(self):
        result = {'coverage': 'sampled-not-lifetime-attribution', 'snapshots': self.snapshots,
                  'missing': self.missing, 'truncated': self.truncated,
                  'roles': {role: {'identities': len(self.identities[role]), 'max_observed': values}
                            for role, values in self.roles.items()}}
        if self.windows_intervals is not None:
            result['windows_intervals'] = self.windows_intervals
        return result
