use numera_core::{cli, ownership::capture};
use serde_json::Value;
use std::{path::Path, process::Command, time::Duration};

#[test]
fn variadic_positionals_preserve_original_option_separated_runs()
-> Result<(), Box<dyn std::error::Error>> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut original = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    original
        .args([
            "-I",
            "-B",
            "-c",
            r#"
import argparse,contextlib,io,json,sys
from pathlib import Path
repository=Path(sys.argv[1])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.cli.parser import build_parser
parser=build_parser()
tails=[['first','--json','second'],['first','second','--json','third'],
       ['--json','first','second'],['first','second','--json'],
       ['first','--json'],['--json'],[],['first','--','second'],
       ['first','--json','--','second'],['--','first','--json','second'],
       ['first','--json','second','-h'],['-h','first','--json','second'],
       ['first','--json','second','--unknown'],
       ['first','--json','second','--json=bad'],
       ['-١٢','--json','second'],['first','--json','-١٢'],
       ['first','--json','--unknown value'],['first','--json','--'],
       ['first','--'],['--'],['--json','--']]
argvs=[]
occurrences=0
def visit(command,path):
    global occurrences
    actions=[a for a in command._actions if not a.option_strings
             and not isinstance(a,argparse._SubParsersAction)]
    if len(actions)==1 and actions[0].nargs in ('*','+'):
        occurrences+=1
        required=[a.option_strings[0]+'=fixture' for a in command._actions
                  if a.option_strings and a.required]
        argvs.extend(path+required+tail for tail in tails)
    for action in command._actions:
        if isinstance(action,argparse._SubParsersAction):
            for name,child in action.choices.items():visit(child,path+[name])
visit(parser,[])
argvs.extend([
    ['delete','first','-y','second'],['delete','-y','first','second'],
    ['apply','first','--notes=second','third'],
    ['apply','--notes=first','second','third'],
    ['apply','first','--notes','second','third'],
    ['apply','first','second','--notes','third'],
    ['pool','export','first','--file=fixture','second'],
    ['pool','export','--file=fixture','first','second'],
])
rows=[]
for argv in argvs:
    with contextlib.redirect_stdout(io.StringIO()),contextlib.redirect_stderr(io.StringIO()):
        try:
            values=vars(parser.parse_args(argv));values.pop('func',None)
            rows.append({'argv':argv,'exit':0,'values':values})
        except SystemExit as error:rows.append({'argv':argv,'exit':error.code})
print(json.dumps({'occurrences':occurrences,'rows':rows},ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut original,
        capture::Limits {
            wall: Duration::from_secs(20),
            idle: Duration::from_secs(15),
            bytes: 1024 * 1024,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original variadic observer failed");
    let document: Value = serde_json::from_slice(&observed.stdout)?;
    let occurrences = document["occurrences"]
        .as_u64()
        .ok_or("Occurrences missing")?;
    assert_eq!(occurrences, 8, "Original variadic inventory differs");
    let cases = document["rows"].as_array().ok_or("Variadic rows missing")?;
    assert_eq!(cases.len() as u64, occurrences * 21 + 8);
    let mut registry = cli::registry();
    registry.build();
    for (index, case) in cases.iter().enumerate() {
        let argv = case["argv"]
            .as_array()
            .ok_or("Variadic arguments missing")?
            .iter()
            .map(|v| v.as_str().ok_or("Argument not text"))
            .collect::<Result<Vec<_>, _>>()?;
        let actual = cli::parse(std::iter::once("numera").chain(argv));
        assert_eq!(
            i64::from(actual.as_ref().err().map_or(0, clap::Error::exit_code)),
            case["exit"].as_i64().ok_or("Original exit missing")?,
            "CLI variadic run admission differs: case={index}"
        );
        if let Ok(matches) = actual {
            assert_eq!(
                cli::reflection::values(&registry, &matches),
                case["values"],
                "CLI variadic value differs: case={index}"
            );
        }
    }
    Ok(())
}
