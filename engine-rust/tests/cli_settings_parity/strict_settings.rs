use super::{Application, BTreeMap, Command, Duration, Path, Value, cli, common, json};
use numera_core::{
    ownership::{capture, registry::Registry},
    storage::repo::RepoError,
};
use std::fs;

#[test]
fn settings_commands_preserve_original_decoding_refusals_and_early_errors()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("strict-settings-cli")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import contextlib,io,json,os,sys
from pathlib import Path
from unittest.mock import patch
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.cli.parser import build_parser
from numera.cli.out import set_json_mode
from numera.store import ProfileStore
from run_logging import run_log
parser=build_parser()
operations=[['engines'],['default-engine'],['default-engine','invalid'],['config'],
            ['config','--default-engine','invalid'],['ext','default-list'],
            ['vault','status'],['vault','enable']]
# These byte sequences exercise decoding, not the valid-text JSON fallback.
settings=[bytes([255]),bytes([0xc3,0x28]),bytes([0xe2,0x82]),b'{}',b'{broken',b'[]',None]
available={'camoufox':False,'cloak':False,'patchright':False,'playwright':True}
observed=[]
with run_log('strict-settings-cli-oracle',parent/'logs') as log:
    for operation in operations:
        for raw in settings:
            root=parent/str(len(observed));store=ProfileStore(root,password=None)
            path=root/'config.json'
            if raw is not None:path.write_bytes(raw)
            args=parser.parse_args(operation+['--json']);set_json_mode(True)
            output=io.StringIO()
            with contextlib.redirect_stdout(output),patch('numera.drivers.available',return_value=available), \
                    patch('numera.drivers.is_installed',side_effect=lambda name:available.get(name,False)):
                try:
                    code=args.func(args,store);payload=json.loads(output.getvalue());error=None
                    if isinstance(payload,dict) and 'root' in payload:
                        assert payload['root']==str(root.resolve())
                        payload['root']='fixture-root'
                except UnicodeDecodeError:code,payload,error=None,None,'UnicodeDecodeError'
                except RuntimeError as exc:code,payload,error=None,None,str(exc)
            assert (path.read_bytes() if path.exists() else None)==raw
            observed.append({'argv':operation,'raw':list(raw) if raw is not None else None,
                             'exit':code,'payload':payload,'error':error})
    log.info('Observed strict settings CLI cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source"));
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        original.exit, 0,
        "Original strict settings CLI observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 56);
    let installed = BTreeMap::from([
        ("camoufox".into(), false),
        ("cloak".into(), false),
        ("patchright".into(), false),
        ("playwright".into(), true),
    ]);
    let mut parser = cli::registry();
    parser.build();
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir(&root)?;
        let path = root.join("config.json");
        let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
        if let Some(raw) = &raw {
            fs::write(&path, raw)?;
        }
        let arguments = case["argv"]
            .as_array()
            .ok_or("CLI arguments missing")?
            .iter()
            .map(|value| value.as_str().ok_or("CLI argument is not text"))
            .collect::<Result<Vec<_>, _>>()?;
        let matches = parser
            .clone()
            .try_get_matches_from(std::iter::once("numera").chain(arguments).chain(["--json"]))?;
        let values = cli::reflection::values(&parser, &matches);
        let args = values.as_object().ok_or("CLI namespace missing")?;
        let repo = cli::application::open_store(&root, args, None)?;
        let sessions = Registry::new(root.clone(), "fixture-host".into(), None)?;
        let services = Application {
            root: &root,
            home: &root,
            repo: &repo,
            installed: &installed,
            host: "fixture-host",
            password: None,
            environment_token: false,
            sessions: &sessions,
            browser: None,
        };
        let result = services.execute(args, 1000.0, |_| panic!("Settings fixture prompted"));
        assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
        let invalid = matches!(&result, Err(RepoError::Io(error))
            if error.kind() == std::io::ErrorKind::InvalidData);
        assert_eq!(
            invalid,
            case["error"] == "UnicodeDecodeError",
            "Strict settings CLI admission differs: case={index} argv={}",
            case["argv"]
        );
        match result {
            Ok(mut outcome) => {
                assert!(
                    case["error"].is_null(),
                    "Original command refused: {}",
                    case["argv"]
                );
                if outcome.payload.get("root").is_some() {
                    assert_eq!(outcome.payload["root"], json!(root));
                    outcome.payload["root"] = "fixture-root".into();
                }
                assert_eq!(
                    json!(outcome.exit),
                    case["exit"],
                    "CLI exit differs: {}",
                    case["argv"]
                );
                assert_eq!(
                    outcome.payload, case["payload"],
                    "CLI payload differs: {}",
                    case["argv"]
                );
            }
            Err(error) if !invalid => {
                assert_eq!(Some(error.to_string().as_str()), case["error"].as_str())
            }
            Err(_) => {}
        }
        assert_eq!(
            fs::read(path).ok(),
            raw,
            "CLI changed settings bytes: {}",
            case["argv"]
        );
    }
    temporary.cleanup()?;
    Ok(())
}
