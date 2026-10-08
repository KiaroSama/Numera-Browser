"""Non-sensitive inputs; expectations always come from source write_contract."""
from __future__ import annotations

import copy
from dataclasses import asdict
from numera import field_policy, write_contract
from numera.fingerprint import generate
from numera.models import Profile


def cases() -> list[dict]:
    found = []
    profile = Profile(name='fixture', fingerprint=generate(seed=42, os='windows'),
                      id='fixture00001', created_at=1700000000.0, updated_at=1700000001.0).to_dict()
    for kind in (write_contract.ENGINE_RECORD, write_contract.DASHBOARD_PATCH):
        for value in (None, True, 5, [], 'text', {}, profile):
            found.append({'name': f'container-{len(found)}', 'input': value, 'kind': kind,
                          'expected': write_contract.profile_problems(value, kind=kind)})
    for table_name, table in field_policy.TABLES.items():
        for field, rule in table.items():
            if rule.kind in field_policy.EXEMPT:
                continue
            for value in (None, True, False, -1, 0, 1, 1.5, 0.25, 0.5, 2**60,
                          '', 'text', [], ['text'], [1], {}, {'url': 'https://example.invalid'}):
                record = copy.deepcopy(profile)
                block = record if table_name == 'Profile' else record.setdefault(table_name.lower(), {})
                if table_name == 'Proxy':
                    record['proxy'] = block = {'server': 'http://127.0.0.1:8080'}
                block[field] = value
                found.append({'name': f'field-{table_name}-{field}-{len(found)}', 'input': record,
                              'kind': write_contract.ENGINE_RECORD,
                              'expected': write_contract.profile_problems(record, kind=write_contract.ENGINE_RECORD)})
    for field in (*write_contract.TEXT_FIELDS, *write_contract.TEXT_LIST_FIELDS,
                  *field_policy.NUMBER_RANGES, 'screen', 'proxy'):
        for value in (None, True, 1.5, -1, 'text', [], {}, ['text'], '1920x1080', '0x0'):
            record = {field: value}
            found.append({'name': f'patch-{field}-{len(found)}', 'input': record,
                          'kind': write_contract.DASHBOARD_PATCH,
                          'expected': write_contract.profile_problems(record, kind=write_contract.DASHBOARD_PATCH)})
    return found


def policy() -> dict:
    return {'tables': {name: {field: {**asdict(rule), 'bounds': rule.bounds()}
                             for field, rule in table.items()}
                       for name, table in field_policy.TABLES.items()},
            'ranges': field_policy.NUMBER_RANGES,
            'texts': write_contract.TEXT_FIELDS, 'text_lists': write_contract.TEXT_LIST_FIELDS,
            'proxy_texts': write_contract.PROXY_TEXT_FIELDS}
