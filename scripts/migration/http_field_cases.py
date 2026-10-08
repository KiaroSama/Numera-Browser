"""Generate the original HTTP write-field schema and refusal observations."""
from __future__ import annotations

import copy
from numera.server import routes_profiles


def document() -> dict:
    fields = {}
    for key, kinds in routes_profiles.FIELD_TYPES.items():
        fields[key] = [kind.__name__ for kind in kinds] if isinstance(kinds, tuple) else [kinds.__name__]
    proxy_fields = {}
    for key, kinds in routes_profiles.PROXY_TYPES.items():
        proxy_fields[key] = [kind.__name__ for kind in kinds] if isinstance(kinds, tuple) else [kinds.__name__]
    values = [None, True, False, '', 'fixture', 0, 1, 1.5, [], ['fixture'], [1], {}, {'pass':17}, 10**100]
    cases = []
    for key in fields:
        for index, value in enumerate(values):
            record = {key: copy.deepcopy(value)}
            cases.append({'name':f'{key}-{index}', 'input':record,
                          'expected':routes_profiles.check_fields(record, where='profile')})
    for record in [{'future':1}, {'future':1,'zzz':2,'name':False}, {'screen':'0x0'}, {'screen':'1920x1080'},
                   {'screen':'1_920x1_080'}, {'screen':'١٩٢٠x١٠٨٠'}, {'proxy':{'port':True}},
                   {'proxy':{'host':1,'pass':True,'country':[]}}, {'addTags':['tag']}, {'_rev':1}]:
        cases.append({'name':f'edge-{len(cases)}','input':record,
                      'expected':routes_profiles.check_fields(record,where='profile')})
    return {'fields':fields,'proxy_fields':proxy_fields,'client_fields':sorted(routes_profiles.client_fields()),
            'server_owned':sorted(routes_profiles.CLIENT_MAY_NOT_SET),'cases':cases}
