use numera_core::{cli, ownership::capture};
use serde_json::Value;
use std::{path::Path, process::Command, time::Duration};

#[test]
fn version_flag_matches_original_root_scope_and_value_admission()
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
import argparse,contextlib,io,json,os,sys
from pathlib import Path
from unittest.mock import patch
repository=Path(sys.argv[1])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.cli.parser import build_parser
parser=build_parser()
argvs=[['-V'],['--version'],['--ver'],['--vers'],['--version='],
       ['--version=fixture'],['-VV'],['-Vh'],['-V=fixture'],['--v'],
       ['--version','list'],['--version','--unknown'],
       ['--version','-V'],['--password=fixture','--version'],
       ['-V','--version'],['-VV','--ver'],['-Vh','--version'],
       ['-V=fixture','--version'],['-V','--help'],['-V','list','--help'],
       ['-V','--password','--version'],['-V','unknown','--version'],
       ['-V','list'],['-V','show','--','-V'],['-V value','--version'],
       ['--help','-V'],['-hV'],['--password','-V','list'],
       ['--password=-V','list'],['--data-dir=-V','list'],
       ['--password=--version','list'],['--data-dir=--version','list'],
       ['--password','--version','list'],['--','-V'],['--','--version'],
       ['list','--version'],['list','-V'],['show','--','-V'],
       ['show','--','--version'],['create','-V'],['create','--','-V'],
       ['edit','fixture','--notes=-V'],['edit','fixture','--notes=--version'],
       ['--help','--version'],['--version','--help'],['--data-dir=fixture','--version'],
       ['--password','-V','list','--help'],['--data-dir','-V','list','--help'],
       ['-V','--unknown','--version'],['-V','--unknown=fixture','--version'],
       ['-V','list','-V','--help'],['-V','list','extra','--help'],
       ['-V','show','fixture','extra','--help'],['-V','list','--tag','-V','--help'],
       ['-V','delete','fixture','-y','--help'],['-V','--password','-V','list','--help'],
       ['-V','show','-1','--help'],['-V','list','-xx','--help'],
       ['-V','list','-xy=fixture','--help'],['-V','create','--co=fixture','--help'],
       ['-V','delete','fixture','-yx','--help'],['-V','list','-xx','--json'],
       ['-V','show','-V','--help'],['-V','list','--tag','-V','--json'],
       ['-V','-1','--version'],['-V','jobs','-1','--help'],
       ['-V','delete','fixture','-y=fixture','--help'],['-V','export','-1','--help'],
       ['-V','delete','fixture','--json','-1','--help'],['-V','show','fixture','-12','--help'],
       ['list','-V','--help'],['-V','--unknown value','--version'],
       ['-V','jobs','--unknown value','--help'],['-V','create','-h'],
       ['-V','delete','fixture','-yh=fixture'],['-V','delete','fixture','-hy=fixture']]
for prefix,option,valid in [(['create','fixture'],'seed','2'),
                            (['warmup','fixture'],'minutes','1.5'),
                            (['create','fixture'],'os','linux')]:
    for value in ['invalid',valid]:
        argvs.append(['-V',*prefix,'--'+option+'='+value,'--help'])
if sys.platform!='win32':
    argvs.extend([['-V',os.fsdecode(b'--unknown=\xff'),'--version'],
                 ['-V','--password',os.fsdecode(b'\xff'),'--version'],
                 ['-V','--password='+os.fsdecode(b'\xff'),'--version']])
rows=[]
help_action=argparse._HelpAction.__call__
for argv in argvs:
    output,error_output=io.StringIO(),io.StringIO()
    helped=[]
    def show_help(action,selected,*args,**kwargs):
        helped.append((selected.prog,args[-1]))
        return help_action(action,selected,*args,**kwargs)
    with patch.object(argparse._HelpAction,'__call__',show_help),contextlib.redirect_stdout(output),contextlib.redirect_stderr(error_output):
        try:
            values=vars(parser.parse_args(argv));values.pop('func',None)
            row={'argv':argv,'exit':0,'values':values}
        except SystemExit as error:row={'argv':argv,'exit':error.code}
    if row['exit']==0 and 'values' not in row:
        if helped:
            assert len(helped)==1
            row['help'],row['help_flag']=helped[0]
        else:
            assert output.getvalue().startswith('numera ')
            row['version']=output.getvalue()
    for name in ('seed','minutes','os'):
        if 'argument --'+name+':' in error_output.getvalue():
            row['invalid_option']='--'+name
    if any(any(0xd800<=ord(c)<=0xdfff for c in arg) for arg in argv):
        row['argv']=[list(os.fsencode(arg)) for arg in argv]
    rows.append(row)
print(json.dumps(rows,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut original,
        capture::Limits {
            wall: Duration::from_secs(15),
            idle: Duration::from_secs(10),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original version flag observer failed");
    let rows: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(
        rows.len(),
        if cfg!(unix) { 85 } else { 82 },
        "Version flag observations missing"
    );
    let mut registry = cli::registry();
    registry.build();
    for (index, row) in rows.iter().enumerate() {
        let argv = row["argv"]
            .as_array()
            .ok_or("Version arguments missing")?
            .iter()
            .map(argument)
            .collect::<Result<Vec<_>, _>>()?;
        let result = cli::parse(std::iter::once(std::ffi::OsString::from("numera")).chain(argv));
        assert_eq!(
            i64::from(result.as_ref().err().map_or(0, clap::Error::exit_code)),
            row["exit"].as_i64().ok_or("Version exit missing")?,
            "CLI version flag admission differs: case={index}"
        );
        match result {
            Ok(matches) => assert_eq!(
                cli::reflection::values(&registry, &matches),
                row["values"],
                "CLI version flag values differ: case={index}"
            ),
            Err(error) => {
                assert!(
                    row.get("values").is_none(),
                    "CLI version flag parsed values missing: case={index}"
                );
                if let Some(option) = row["invalid_option"].as_str() {
                    assert!(
                        error.to_string().contains(option),
                        "CLI argument error precedence differs: case={index}"
                    );
                }
                if let Some(scope) = row["help"].as_str() {
                    assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
                    let usage = format!("Usage: {scope}");
                    let text = error.to_string();
                    let mut selected = &registry;
                    for name in scope.split_whitespace().skip(1) {
                        selected = selected
                            .find_subcommand(name)
                            .ok_or("Help command missing")?;
                    }
                    let expected = selected
                        .clone()
                        .try_get_matches_from([
                            "numera",
                            row["help_flag"].as_str().ok_or("Help flag missing")?,
                        ])
                        .expect_err("Help must return a display");
                    assert_eq!(
                        text,
                        expected.to_string(),
                        "CLI help rendering differs: case={index}"
                    );
                    assert!(
                        text.lines()
                            .any(|line| line
                                .strip_prefix(&usage)
                                .is_some_and(|rest| rest.is_empty()
                                    || rest.trim_start().starts_with(['[', '<']))),
                        "CLI help scope differs: case={index}"
                    );
                }
                if let Some(version) = row["version"].as_str() {
                    assert_eq!(error.kind(), clap::error::ErrorKind::DisplayVersion);
                    assert_eq!(
                        error.to_string(),
                        version,
                        "CLI version output differs: case={index}"
                    );
                }
            }
        }
    }
    Ok(())
}

fn argument(value: &Value) -> Result<std::ffi::OsString, Box<dyn std::error::Error>> {
    if let Some(text) = value.as_str() {
        return Ok(text.into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(std::ffi::OsString::from_vec(serde_json::from_value(
            value.clone(),
        )?))
    }
    #[cfg(not(unix))]
    Err("Native-byte arguments require Unix".into())
}
