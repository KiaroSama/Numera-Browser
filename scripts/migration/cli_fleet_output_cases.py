"""Original fleet/import narration at the observed import boundary."""
from __future__ import annotations

import io
from types import SimpleNamespace
from unittest.mock import patch
from numera.cli.parser import build_parser
from numera.cli.out import set_json_mode
from numera.cli import profiles as handlers


def cases():
    class Stream(io.StringIO):
        def __init__(self,tty): super().__init__();self.tty=tty
        def isatty(self): return self.tty
    variants=[('empty',[],[]),('clean',[(SimpleNamespace(id='one',name='Fixture'),[])],[]),
              ('review',[(SimpleNamespace(id='one',name='Fixture ع 😀'),['Controlled issue']),
                         (SimpleNamespace(id='two',name='Clean'),[])],['Taken','Taken','Another']),('unreadable',[],[])]
    result=[];parser=build_parser()
    for tty in [False,True]:
        for variant,imported,skipped in variants:
            args=parser.parse_args(['import-from','fixture-file']);stdout=Stream(tty);payload=[];set_json_mode(False)
            def emit(value,code=0): payload.append(value);return code
            def read(*args):
                if variant=='unreadable': raise ValueError('Controlled unreadable export')
                return imported,skipped
            with patch('sys.stdout',stdout),patch.object(handlers,'emit',side_effect=emit),patch('numera.importers.import_file',side_effect=read):
                code=args.func(args,SimpleNamespace())
            result.append({'command':'import-from','tty':tty,'direct':True,'fleet':True,'payload':payload[0],'stdout':stdout.getvalue(),'exit':code})
    return result
