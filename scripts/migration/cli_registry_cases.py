"""Observe the original command tree, MCP schemas and parser namespace only."""
from __future__ import annotations

import argparse
import contextlib
import io
from baseline_source import activate
activate()

from numera.cli.parser import build_parser
from numera import mcp


def cases():
    parser = build_parser()
    leaves = list(mcp._walk(parser, (), {}))
    samples = []
    for path, leaf, _help in leaves:
        argv = list(path)
        for action in leaf._actions:
            if isinstance(action, argparse._SubParsersAction) or action.dest == 'help':
                continue
            if not action.option_strings and action.nargs != '*':
                argv.append('fixture')
            elif action.required:
                argv.extend([action.option_strings[-1], '2' if action.type is int else '1.25' if action.type is float else 'fixture'])
        samples.append(argv)
    samples.extend([
        ['create','fixture','--seed','١_٢٣','--tag','one','--tag','two','--json'],
        ['create','fixture','--seed','999999999999999999999999999999999999'],
        ['launch','fixture','--headless','--headless'],
        ['launch','fixture','--headl'], ['create','fixture','--not','text'],
        ['create','fixture','--co','US'],
        ['config','--default','playwright'],
        ['edit','fixture','--notes','old','--notes','new'],
        ['--data-dir','fixture-root','--password','synthetic','pool','edit','a','--pass','synthetic-proxy'],
        ['delete','a','b','-y','--json'],
        ['pool','export'], ['pool','test','--all'], ['apply','--all','--extension',''],
        ['schedule','add','fixture','--every','١.٥'],
        ['warmup','fixture','--minutes','1_2.5'],
        [], ['create'], ['create','fixture','--os','ios'],
        ['create','fixture','--seed','1.2'], ['create','fixture','--seed','1__2'],
        ['schedule','add','fixture'], ['pool','remove'], ['serve','--port','bad'],
    ])
    results = []
    for argv in samples:
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            try:
                namespace = vars(parser.parse_args(argv))
                namespace.pop('func', None)
                results.append({'argv':argv, 'values':namespace, 'exit':0})
            except SystemExit as error:
                results.append({'argv':argv, 'exit':error.code})
    argument_cases = []
    for path, leaf, _help in leaves:
        if path in mcp._NOT_TOOLS:
            continue
        arguments = {}
        for action in leaf._actions:
            if isinstance(action, (argparse._HelpAction, argparse._SubParsersAction)) or any(opt in mcp._MANAGED for opt in action.option_strings):
                continue
            if isinstance(action, argparse._StoreTrueAction):
                value = True
            elif isinstance(action, argparse._AppendAction) or action.nargs in ('*','+'):
                value = ['one','two']
            elif action.type is int:
                value = 2
            elif action.type is float:
                value = 1.25
            elif action.choices:
                value = list(action.choices)[0]
            else:
                value = 'fixture'
            arguments[action.dest] = value
        name = '_'.join(path).replace('-','_')
        argument_cases.append({'name':name, 'arguments':arguments, 'argv':mcp._argv(path,leaf,arguments)})
    return {'tools':mcp.tools(), 'parse':results, 'argv':argument_cases}


def output_cases():
    from numera.cli import out
    from unittest.mock import patch
    result=[]
    class Stream(io.StringIO):
        def __init__(self,tty): super().__init__();self.tty=tty
        def isatty(self): return self.tty
    for json_mode,tty,text in [(False,False,'y\n'),(False,True,'N\n'),(False,False,''),(True,False,'must-not-read')]:
        stdout=Stream(tty);stderr=Stream(tty);error=None;confirmed=None
        with patch('sys.stdout',stdout),patch('sys.stderr',stderr),patch('sys.stdin',io.StringIO(text)):
            out.set_json_mode(json_mode)
            out.say(out._c('مرحبا Fixture',out.C.CYN))
            try: confirmed=out._confirm('Continue? [y/N] ')
            except RuntimeError as exception: error=str(exception)
            code=out.emit({'text':'مرحبا 😀','values':[True,None,123],'nested':{'key':'value'}},7)
        result.append({'json':json_mode,'tty':tty,'input':text,'stdout':stdout.getvalue(),'stderr':stderr.getvalue(),
                       'confirmed':confirmed,'error':error,'exit':code})
    out.set_json_mode(False)
    return result
