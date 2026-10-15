"""Original bulk identity/patch/selection observations, no application writes."""
from __future__ import annotations
from unittest.mock import patch
from baseline_source import activate
activate()

from numera import bulk
from numera.fingerprint import generate
from numera.models import Profile


def cases():
    generation=[]
    for options in [dict(count=0,seed=42),dict(count=3,seed=42),
                    dict(count=4,seed=-7,oses=['windows','macos'],locales=['en-US','de-DE'],chrome_versions=['128.0.0.0','129.0.0.0'],start_index=-1,pad=4,tags=['one']),
                    dict(count=2,seed=7,oses=['android'],mobile=True,name_prefix='Mobile')]:
        ids=iter(range(10))
        def profile(**kwargs):
            return Profile(**kwargs,id=f'fixture{next(ids)}',created_at=1000.0,updated_at=1000.0)
        with patch.object(bulk,'Profile',side_effect=profile):
            profiles=bulk.generate_profiles(**options)
        generation.append({'options':options,'expected':[p.to_dict() for p in profiles]})
    base=Profile(id='fixture',name='Fixture',fingerprint=generate(seed=42),tags=['one'],created_at=1000.0,updated_at=1000.0)
    changes=[]
    for changeset in [{},{'engine':'camoufox','notes':'changed','startup_urls':['about:blank'],'extensions':['folder']},
                      {'add_tags':['two','two'],'remove_tags':['one']},
                      {'locale':'de-DE','timezone':'UTC'}, {'locale':'unknown'}, {'regen':True}]:
        profile=Profile.from_dict(base.to_dict())
        original_generate=generate
        with patch.object(bulk,'generate',side_effect=lambda **kwargs:original_generate(seed=123,**kwargs)):
            changed=bulk.apply_patch(profile,changeset)
        changes.append({'input':base.to_dict(),'patch':changeset,'changed':changed,'expected':profile.to_dict()})
    second=Profile(id='second',name='Second',fingerprint=generate(seed=43),tags=['two'],created_at=1000.0,updated_at=1000.0)
    selections=[]
    for options in [dict(),dict(refs=['Fixture']),dict(tag='two'),dict(refs=['second'],tag='one'),dict(everything=True)]:
        selections.append({'options':options,'expected':[p.id for p in bulk.select([base,second],**options)]})
    return {'generation':generation,'changes':changes,'profiles':[base.to_dict(),second.to_dict()],'selections':selections}
