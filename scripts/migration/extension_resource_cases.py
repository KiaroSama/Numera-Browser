"""Original installed-extension labels and contained icon selection; no downloads."""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from numera import webstore


def cases(parent: Path) -> list[dict]:
    parent.mkdir(parents=True, exist_ok=True)
    found = []
    variants = (
        ('plain', {'name':'Extension α','icons':{'16':'small.png','128':'large.png'}}, {}),
        ('action', {'name':'Action','action':{'default_icon':'action.png'}}, {}),
        ('fallback', {'name':'Fallback','icons':{'128':'missing.png','16':'small.png'}}, {}),
        ('localized', {'name':'__MSG_title__','default_locale':'fr'}, {'fr':{'title':{'message':'Localized α'}}}),
        ('english-first', {'name':'__MSG_title__','default_locale':'fr'}, {'en':{'title':{'message':'English'}},'fr':{'title':{'message':'French'}}}),
        ('unresolved', {'name':'__MSG_missing__'}, {}),
        ('outside-icon', {'name':'Outside','icons':{'128':'../outside.png','16':'small.png'}}, {}),
        ('bad-manifest', None, {}),
    )
    for label,manifest,locales in variants:
        with tempfile.TemporaryDirectory(prefix='extension-oracle-',dir=parent) as directory:
            root = Path(directory)
            folder = root / 'extensions/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
            folder.mkdir(parents=True)
            files = {'manifest.json':json.dumps(manifest) if manifest is not None else '{ broken',
                     'small.png':'fixture-small','large.png':'fixture-large','action.png':'fixture-action'}
            for locale,messages in locales.items(): files[f'_locales/{locale}/messages.json'] = json.dumps(messages)
            for name,value in files.items():
                path = folder / name
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_text(value,encoding='utf-8')
            (root / 'extensions/outside.png').write_text('outside',encoding='utf-8')
            description = webstore.describe(folder)
            icon = Path(description['icon']).relative_to(folder).as_posix() if description['icon'] else None
            installed = webstore.installed(root)
            found.append({'name':label,'files':files,'expected_name':description['name'],'icon':icon,
                          'installed_ids':[entry['id'] for entry in installed]})
    return found
