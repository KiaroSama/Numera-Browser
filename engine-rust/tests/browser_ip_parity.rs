use numera_core::{browser::ip_identity, models::Profile};
use serde_json::Value;

#[test]
fn only_marked_identity_signals_follow_observed_exit_country()
-> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/browser-ip.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("IP identity cases missing")?;
    assert_eq!(cases.len(), 33);
    for case in cases {
        let profile = Profile::from_record(&case["input"])?;
        assert_eq!(
            u64::from(ip_identity::needs_lookup(&profile)),
            case["lookups"].as_u64().ok_or("Lookup count missing")?
        );
        let actual = ip_identity::resolve(&profile, case["country"].as_str())?;
        assert_eq!(
            serde_json::to_value(actual)?,
            case["expected"],
            "IP identity {}",
            case["name"]
        );
    }
    Ok(())
}

mod common;

#[test]
fn timezone_grades_match_original_proxy_and_trust_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{
        browser::{network_grades, trust_grades},
        ownership::capture,
    };
    use serde_json::json;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("timezone-grades")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,sys
from contextlib import contextmanager
from pathlib import Path
from unittest.mock import patch
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.fingerprint import generate
from numera.models import Profile
from numera.probe import network,trust
from numera.probe.dns import _DNS_JS
from run_logging import run_log
profile=Profile(name='Fixture',id='fixture',fingerprint=generate(seed=42))
fp=profile.fingerprint
seen={'webdriver':False,'webdriverOwn':False,'hasChrome':True,'plugins':5,
      'userAgent':fp.user_agent,'platform':fp.platform,'languages':fp.languages,
      'cores':fp.hardware_concurrency,'memory':fp.device_memory,'timezone':fp.timezone,
      'toStringNative':True,'permissionsAgree':True,'canvasStable':True,
      'webglRenderer':fp.webgl_renderer,'fontKnown':True,'fontAlien':False,
      'cameras':fp.cameras,'microphones':fp.microphones}
base_profile=profile.to_dict()
base_seen=copy.deepcopy(seen)
whitespace=[9,10,11,12,13,28,29,30,31,32,133,160,5760,8192,8193,8194,8195,
            8196,8197,8198,8199,8200,8201,8202,8232,8233,8239,8287,12288]
pairs=[(chr(code)+left+chr(code),right) for code in whitespace
       for left,right in [('Europe/Berlin','Europe/Berlin'),
                          ('Asia/Calcutta','Asia/Kolkata'),('Asia/Kolkata','Asia/Calcutta')]]
pairs += [('Europe/Berlin',chr(code)+'Europe/Berlin'+chr(code)) for code in [28,29,30,31]]
pairs += [('',''),('','Europe/Berlin'),('Europe/Berlin',''),('EUROPE/BERLIN','europe/berlin'),
          ('Europe/Berlin','Europe/Paris'),('Unknown/Zone','unknown/zone'),
          ('Asia/Calcutta','Asia/Kolkata'),('Asia/Kolkata','Asia/Calcutta'),
          ('Europe/\u001cBerlin','Europe/Berlin'),
          (chr(0x200b)+'Europe/Berlin'+chr(0x200b),'Europe/Berlin'),
          (chr(0xfeff)+'Europe/Berlin'+chr(0xfeff),'Europe/Berlin'),('\u001c','\u001f')]
ice={'supported':True,'ips':[]}
dns={'rows':[]}
tls={'ja3':None,'ja3Hash':'fixture','ja4':'t13d1516h2_fixture','http2':None,
     'grease':True,'greaseKnown':True,'cipherCount':15}
rows=[]
with run_log('timezone-grades-oracle',root/'logs') as log:
    for left,right in pairs:
        profile.fingerprint.timezone=right
        seen['timezone']=left
        info={'ip':'203.0.113.1','country':None,'city':None,'timezone':left}
        class Page:
            def goto(self,*args,**kwargs): pass
            def evaluate(self,expression,argument=None):
                if expression==network._IP_JS: return info
                if expression==network._WEBRTC_JS: return ice
                if expression==_DNS_JS: return dns
                if expression==network._TLS_PARSE: return tls
                raise AssertionError('Unexpected original probe expression')
        @contextmanager
        def session(*args,**kwargs):
            yield type('Context',(),{'pages':[Page()]})()
        before_profile=copy.deepcopy(profile.to_dict())
        before_seen=copy.deepcopy(seen)
        with patch.object(network,'session',session),patch.object(network.time,'time',side_effect=[1000,1000.125]):
            proxy=network.check_proxy(profile,None)
        graded=trust._grade(profile.fingerprint,seen,'playwright')
        assert profile.to_dict()==before_profile and seen==before_seen
        rows.append({'left':left,'right':right,'proxy':proxy,'trust':graded})
    log.info('Observed timezone grade pairs=%s',len(rows))
    print(json.dumps({'profile':base_profile,'seen':base_seen,'ice':ice,'dns':dns,'tls':tls,'rows':rows},ensure_ascii=True))
"#]).arg(repository).arg(temporary.path()).env("PYTHONUTF8", "1");
    let observed = match capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 1048576,
            cleanup: Duration::from_secs(5),
        },
    ) {
        Ok(output) => output,
        Err(mut error) => {
            if error.settle(Duration::from_secs(5)).is_err() {
                eprintln!(
                    "Timezone observer unresolved; fixture retained at {}",
                    temporary.path().display()
                );
                std::mem::forget(temporary);
            }
            return Err(error.into());
        }
    };
    assert_eq!(
        observed.exit,
        0,
        "Original timezone observer failed: {}",
        String::from_utf8_lossy(&observed.stderr)
    );
    let document: Value = serde_json::from_slice(&observed.stdout)?;
    let rows = document["rows"].as_array().ok_or("Timezone rows missing")?;
    assert_eq!(rows.len(), 103, "Timezone observations missing");
    let leaks = network_grades::leaks(&document["ice"], Some("203.0.113.1"))?;
    let dns = numera_core::browser::probe_grades::dns(&document["dns"], None)?;
    let tls = numera_core::browser::probe_grades::tls(&document["tls"])?;
    for (index, row) in rows.iter().enumerate() {
        let mut profile = Profile::from_record(&document["profile"])?;
        profile.fingerprint.timezone = row["right"]
            .as_str()
            .ok_or("Profile timezone missing")?
            .into();
        let mut seen = document["seen"].clone();
        seen["timezone"] = row["left"].clone();
        let before_profile = serde_json::to_value(&profile)?;
        let before_seen = seen.clone();
        let info = json!({"ip":"203.0.113.1","country":null,"city":null,"timezone":row["left"]});
        assert_eq!(
            network_grades::proxy(&profile, &info, &leaks, &dns, &tls, 125)?,
            row["proxy"],
            "Timezone proxy grade differs: {index}"
        );
        assert_eq!(
            trust_grades::grade(&profile.fingerprint, &seen, "playwright")?,
            row["trust"],
            "Timezone trust grade differs: {index}"
        );
        assert_eq!(serde_json::to_value(&profile)?, before_profile);
        assert_eq!(seen, before_seen);
    }
    temporary.cleanup()?;
    Ok(())
}
