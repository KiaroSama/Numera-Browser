"""Retained clear-before-seed browser files with isolated synthetic structures."""
from __future__ import annotations
import json
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch
from numera import launch_options
from numera.fingerprint import generate
from numera.models import Profile


def cases(parent: Path):
    parent.mkdir(parents=True, exist_ok=True)
    definitions = [
        ('keep', [], [], {'Default/History':'fixture history','Default/Network/Cookies':'fixture cookies'}),
        ('clear-history', ['history'], [], {'Default/History':'fixture history','Default/History-journal':'fixture journal','Default/Login Data':'fixture login'}),
        ('clear-directory', ['localStorage','unknown'], [], {'Default/Local Storage/leveldb/data':'fixture data','Default/Network/Cookies':'fixture cookies'}),
        ('seed', [], [{'name':'Unicode α فارسی','url':'https://a.test'}], {}),
        ('merge', [], [{'name':'A','url':'https://a.test'},{'name':'B','url':'https://b.test'}], {'Default/Bookmarks':json.dumps({'roots':{'bookmark_bar':{'id':'1','children':[{'id':'99','name':'Mine','url':'https://mine.test','type':'url'}]}},'version':1,'checksum':'old','future':True})}),
        ('corrupt', [], [{'url':'https://a.test'}], {'Default/Bookmarks':'{broken'}),
        ('clear-before-seed', ['bookmarks'], [{'name':'A','url':'https://a.test'}], {'Default/Bookmarks':'{broken','Default/Bookmarks.bak':'fixture backup'}),
        ('duplicates', [], [{'url':'https://a.test'},{'url':'https://a.test'}], {}),
        ('nested-preserved', [], [{'url':'https://a.test'}], {'Default/Bookmarks':json.dumps({'roots':{'bookmark_bar':{'id':'50','children':[{'id':'8','type':'folder','children':[{'id':'80','url':'https://nested.test'}]}]}},'version':1})}),
    ]
    result = []
    for name, clear, bookmarks, files in definitions:
        profile = Profile(id='a', name='fixture', created_at=1000.0, updated_at=1000.0,
                          fingerprint=generate(seed=42, os='windows'),
                          clear_on_start=clear, bookmarks=bookmarks)
        with TemporaryDirectory(dir=parent) as tmp, patch('time.time', lambda:1000.0):
            root = Path(tmp)
            for rel, content in files.items():
                target = root/rel
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(content, encoding='utf-8')
            launch_options._clear_unkept(profile, root)
            launch_options._seed_bookmarks(profile, root)
            written = {}
            for path in sorted(root.rglob('*')):
                if path.is_file():
                    rel = path.relative_to(root).as_posix()
                    content = path.read_text(encoding='utf-8')
                    written[rel] = json.loads(content) if rel=='Default/Bookmarks' else content
            result.append({'name':name,'input':profile.to_dict(),'files':files,'expected':written})
    return result
