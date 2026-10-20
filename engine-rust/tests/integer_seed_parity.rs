use numera_core::{
    cli::{self, profile_commands::Profiles},
    fingerprint::{GenerateOptions, generate},
    models::Profile,
    ownership::capture,
    storage::{config::ConfigStore, repo::ProfileRepo},
};
use serde_json::{Number, Value, json};
use std::{collections::BTreeMap, fs, path::Path, process::Command, time::Duration};
mod common;

#[test]
fn complete_integer_seed_identities_and_commands_match_original()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("integer-seed")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import contextlib,copy,io,json,sys
from pathlib import Path
from unittest.mock import patch
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.fingerprint import generate
from numera.models import Profile
from numera.store import ProfileStore
from numera.cli import profiles as handlers
from numera.cli.parser import build_parser
from numera.cli.out import set_json_mode
from numera.write_contract import WriteRefused
from numera.convert import to_dashboard_profile,to_engine_profile
from numera.collision import find_collisions
from numera.stealth import build_init_script
from run_logging import run_log
seeds=[0,1,-1,2**32-1,2**32,2**53,2**53+1,2**63-1,2**63,
       -2**63-1,2**64,2**200+2**97+17,-(2**200+2**97+17)]
with run_log('integer-seed-oracle',root/'logs') as log:
    rows=[]
    parser=build_parser()
    for index,seed in enumerate(seeds):
        for verb in ('bulk-create','create'):
            store=ProfileStore(root/f'{index}-{verb}')
            config=store.root/'config.json'
            config.write_text('{"default_engine":"camoufox","extra":{"keep":true}}',encoding='utf-8')
            before=config.read_bytes()
            argv=([verb,'Fixture'] if verb=='create' else [verb,'--count','2'])
            argv+=['--seed',str(seed),'--os','windows','--locale','en-US','--engine','playwright','--json']
            args=parser.parse_args(argv)
            ids=iter(('fixture0','fixture1'))
            def profile(**kwargs):
                return Profile(**kwargs,id=next(ids),created_at=1000.0,updated_at=1000.0)
            stream=io.StringIO()
            set_json_mode(True)
            error=None
            code=None
            with contextlib.redirect_stdout(stream),patch.object(handlers,'Profile',side_effect=profile), \
                    patch('numera.bulk.Profile',side_effect=profile), \
                    patch('numera.drivers.is_installed',return_value=True),patch('time.time',return_value=1000.0):
                try:
                    code=args.func(args,store)
                except WriteRefused as refused:
                    error={'type':type(refused).__name__,'message':str(refused)}
            records={kind:[json.loads(path.read_text(encoding='utf-8'))
                           for path in sorted((store.root/kind).glob('*.json'))]
                     for kind in ('profiles','dashboard')}
            rows.append({'argv':argv,'exit':code,'error':error,
                         'payload':json.loads(stream.getvalue()) if stream.getvalue() else None,
                         'records':records,'config':json.loads(config.read_text(encoding='utf-8')),
                         'configUnchanged':config.read_bytes()==before})
    # This limit change is observer transport only, AFTER all real CLI parsing.
    # The >624-word case exercises pure generation, not the CLI's digit limit.
    sys.set_int_max_str_digits(0)
    identities=[]
    for seed in seeds+[2**20000+42]:
        fp=generate(seed=seed,os='windows',locale='en-US')
        profile=Profile(id='fixture0',name='Fixture',fingerprint=fp,created_at=1000.0,updated_at=1000.0)
        twin=copy.deepcopy(profile)
        twin.id='fixture1'
        twin.fingerprint.seed=seed+1
        same=copy.deepcopy(profile)
        same.id='fixture1'
        dash=to_dashboard_profile(profile)
        script=build_init_script(fp)
        encoded=script.split('  const cfg = ',1)[1].split(';\n',1)[0]
        with patch('numera.convert.Profile',side_effect=lambda **kwargs: Profile(
                **kwargs,created_at=1000.0,updated_at=1000.0)):
            converted=to_engine_profile(dash).to_dict()
        identities.append({'seed':seed,'fingerprint':fp.to_dict(),'profile':profile.to_dict(),
                           'dashboard':dash,'converted':converted,
                           'config':json.loads(encoded),'collisions':find_collisions([profile,twin]),
                           'identical':find_collisions([profile,same])})
    log.info('Observed integer identities=%s commands=%s',len(identities),len(rows))
    print(json.dumps({'identities':identities,'commands':rows},ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("original"))
        .env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 1_048_576,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original integer-seed observer failed");
    let document: Value = serde_json::from_slice(&observed.stdout)?;
    let installed = BTreeMap::from([("playwright".into(), true)]);
    let mut registry = cli::registry();
    registry.build();
    let commands = document["commands"].as_array().ok_or("Commands missing")?;
    assert_eq!(commands.len(), 26);
    let mut refused = 0;
    for (index, case) in commands.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let repo = ProfileRepo::open(root.clone(), None, None)?;
        let config = ConfigStore::new(root.clone());
        let config_path = root.join("config.json");
        let initial = b"{\"default_engine\":\"camoufox\",\"extra\":{\"keep\":true}}";
        fs::write(&config_path, initial)?;
        let services = Profiles {
            repo: &repo,
            config: &config,
            installed: &installed,
            host: "fixture",
        };
        let argv = case["argv"]
            .as_array()
            .ok_or("Arguments missing")?
            .iter()
            .map(|v| v.as_str().ok_or("Argument not text"))
            .collect::<Result<Vec<_>, _>>()?;
        let matches = registry
            .clone()
            .try_get_matches_from(std::iter::once("numera").chain(argv))?;
        let values = cli::reflection::values(&registry, &matches);
        let args = values.as_object().ok_or("Parsed arguments missing")?;
        let result = if values["command"] == "create" {
            services.create(args, 1000.0)
        } else {
            services.bulk_create(args, 1000.0)
        };
        let mut payload = if case["error"].is_null() {
            // This assertion must reject the historical bulk i64 guard.
            assert!(
                result.is_ok(),
                "Integer seed command admission differs: {}",
                case["argv"]
            );
            let outcome = result?;
            assert_eq!(
                i64::from(outcome.exit),
                case["exit"].as_i64().ok_or("Exit missing")?
            );
            outcome.payload
        } else {
            refused += 1;
            assert_eq!(case["error"]["type"], "WriteRefused");
            let message = match result {
                Err(error) => error.to_string(),
                Ok(_) => panic!("Invalid persisted seed was accepted"),
            };
            assert_eq!(
                message, case["error"]["message"],
                "Integer seed write refusal"
            );
            Value::Null
        };
        // Observe the write before any repository read can reconcile its twins.
        let mut records = json!({});
        for kind in ["profiles", "dashboard"] {
            let path = root.join(kind);
            let mut rows = Vec::<Value>::new();
            if path.exists() {
                for entry in fs::read_dir(path)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|ext| ext == "json") {
                        rows.push(serde_json::from_slice(&fs::read(path)?)?);
                    }
                }
            }
            rows.sort_by_key(|record| record["name"].to_string());
            records[kind] = Value::Array(rows);
        }
        let ids = records["profiles"]
            .as_array()
            .ok_or("Written records missing")?
            .iter()
            .enumerate()
            .map(|(index, record)| {
                record["id"]
                    .as_str()
                    .map(|id| (id.to_owned(), format!("fixture{index}")))
                    .ok_or("Written profile ID missing")
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        normalize_ids(&mut payload, &ids);
        normalize_ids(&mut records, &ids);
        assert_eq!(payload, case["payload"], "Integer seed command payload");
        assert_eq!(records, case["records"], "Integer seed persisted records");
        assert_eq!(Value::Object(config.read()?), case["config"]);
        assert_eq!(
            json!(fs::read(config_path)? == initial),
            case["configUnchanged"]
        );
    }
    assert_eq!(refused, 8, "Original write-policy boundary coverage");
    let identities = document["identities"]
        .as_array()
        .ok_or("Identities missing")?;
    assert_eq!(identities.len(), 14);
    for case in identities {
        let seed: Number = serde_json::from_value(case["seed"].clone())?;
        let fingerprint = generate(GenerateOptions {
            seed,
            os: Some("windows"),
            locale: Some("en-US"),
            country: None,
            mobile: false,
        })?;
        assert_eq!(
            serde_json::to_value(&fingerprint)?,
            case["fingerprint"],
            "Complete integer identity"
        );
        assert_eq!(
            numera_core::identity_script::config(&fingerprint)?,
            case["config"]
        );
        let profile = Profile::from_record(&json!({"id":"fixture0", "name":"Fixture",
            "fingerprint":fingerprint, "created_at":1000.0,"updated_at":1000.0}))?;
        assert_eq!(serde_json::to_value(&profile)?, case["profile"]);
        let dashboard = numera_core::conversion::to_dashboard(&profile);
        assert_eq!(dashboard, case["dashboard"]);
        assert_eq!(
            serde_json::to_value(numera_core::conversion::to_engine(&dashboard, 1000.0)?)?,
            case["converted"]
        );
        let mut twin = profile.clone();
        twin.id = "fixture1".into();
        twin.fingerprint.seed =
            serde_json::from_value(case["collisions"][0]["members"][1]["seed"].clone())?;
        assert_eq!(
            json!(numera_core::collision::find(&[
                profile.clone(),
                twin.clone()
            ])),
            case["collisions"]
        );
        twin.fingerprint.seed = profile.fingerprint.seed.clone();
        assert_eq!(
            json!(numera_core::collision::find(&[profile, twin])),
            case["identical"]
        );
    }
    for text in ["1.0", "1e20", "-0.5"] {
        let seed: Number = serde_json::from_str(text)?;
        assert!(
            generate(GenerateOptions {
                seed,
                os: None,
                locale: None,
                country: None,
                mobile: false,
            })
            .is_err()
        );
    }
    temporary.cleanup()?;
    Ok(())
}

fn normalize_ids(value: &mut Value, ids: &BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            if let Some(replacement) = ids.get(text) {
                *text = replacement.clone();
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| normalize_ids(v, ids)),
        Value::Object(items) => items.values_mut().for_each(|v| normalize_ids(v, ids)),
        _ => {}
    }
}
