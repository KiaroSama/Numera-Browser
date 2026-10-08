"""Retained identity init script and configuration from original generator."""
from __future__ import annotations
import json
from numera.fingerprint import generate
from numera.stealth import build_init_script


def document()->dict:
    cases=[]
    template=None
    for os in ['windows','macos','linux','android','ios']:
        for seed in [1,42]:
            fingerprint=generate(seed=seed,os=os,locale='en-US')
            script=build_init_script(fingerprint)
            prefix,rest=script.split('  const cfg = ',1)
            encoded,suffix=rest.split(';\n',1)
            current=prefix+'  const cfg = {{CONFIG}};\n'+suffix
            if template is not None and template!=current:
                raise ValueError('Original script template varies unexpectedly')
            template=current
            cases.append({'name':f'{os}-{seed}','input':fingerprint.to_dict(),'config':json.loads(encoded),'script':script})
    return {'template':template,'cases':cases}
