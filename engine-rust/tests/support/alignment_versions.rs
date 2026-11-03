use numera_core::{browser::alignment, models::Profile, ownership::capture};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};

#[test]
fn alignment_versions_match_original_decimal_captures() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::common::TempDirectory::new("alignment-versions")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,sys
from pathlib import Path
from unittest.mock import patch
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.fingerprint import generate
from numera.models import Profile
from numera.probe import align,trust
from run_logging import run_log
profile=Profile(name='Fixture',id='fixture',fingerprint=generate(seed=42,os='windows'))
base=profile.to_dict()
fp=profile.fingerprint
seen={'webdriver':False,'webdriverOwn':False,'hasChrome':True,'plugins':5,
      'userAgent':fp.user_agent,'platform':fp.platform,'languages':fp.languages,
      'cores':fp.hardware_concurrency,'memory':fp.device_memory,'timezone':fp.timezone,
      'toStringNative':True,'permissionsAgree':True,'canvasStable':True,
      'webglRenderer':fp.webgl_renderer,'fontKnown':True,'fontAlien':False,
      'cameras':fp.cameras,'microphones':fp.microphones,'fontsRendered':fp.fonts}
ranges=json.loads((repository/'engine-rust/src/pool_digit_ranges.json').read_text(encoding='utf-8'))['decimal_ranges']
inputs=['Chrome/'+'.'.join([chr(lo),chr(hi),chr(lo)+chr(hi),'0'+chr(lo)]) for lo,hi in ranges]
inputs += ['',fp.user_agent,'Chrome/1.2.3.4','Chrome/001.02.003.0045',
           'Chrome/1.2.3.4.5','Chrome/1.2.3','Chrome/.1.2.3','Chrome/1..2.3.4',
           'chrome/1.2.3.4','Chromium/1.2.3.4','Chrome/+1.2.3.4','Chrome/1.2.3.-4',
           'Chrome/1.2.3.4suffix','Chrome/1.2.3.x Chrome/5.6.7.8',
           'Chrome/1.2.3.Chrome/5.6.7.8','Chrome/1.2.3.4 Chrome/5.6.7.8',
           'Chrome/'+'.'.join(['9'*80]*4),'Chrome/1\n.2.3.4',
           'Chrome/1.2.3.4\x00Chrome/5.6.7.8']
inputs += ['Chrome/'+'.'.join([chr(code)]*4) for code in [0x2165,0x3039,0x2082,0x32b4]]
inputs += [chr(0x1f642)+'Chrome/'+chr(0x6f1)+'.2.3.4',
           'Chrome/'+chr(0x6f1)+'.2.3.4 Chrome/5.6.7.8',
           'Chrome/1.2.3.x Chrome/'+chr(0x6f1)+'.2.3.4',
           'Chrome/'+chr(0x6f1)+chr(0x200b)+'.2.3.4',
           'Chrome/1'+chr(0x066b)+'2.3.4']
rows=[]
class Repository:
    def __init__(self,current):
        self.current=current
        self.calls=0
    def edit(self,identifier,change):
        assert identifier==self.current.id
        self.calls+=1
        self.current=change(self.current)
        return self.current
with run_log('alignment-versions-oracle',root/'logs') as log:
    for ua in inputs:
        current=Profile.from_dict(copy.deepcopy(base))
        observation={**copy.deepcopy(seen),'userAgent':ua}
        before_seen=copy.deepcopy(observation)
        owned=Repository(current)
        with patch.object(trust,'_observe',return_value=observation),patch('numera.repo.repo',return_value=owned):
            result=align.align_to_reality(current,None)
        assert owned.calls==1 and observation==before_seen
        rows.append({'ua':ua,'fingerprint':owned.current.to_dict()['fingerprint'],'changed':result['changed']})
    log.info('Observed alignment versions=%s',len(rows))
    print(json.dumps({'profile':base,'seen':seen,'ranges':len(ranges),'rows':rows},ensure_ascii=True))
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
                    "Alignment observer unresolved; fixture retained at {}",
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
        "Original alignment observer failed: {}",
        String::from_utf8_lossy(&observed.stderr)
    );
    let document: Value = serde_json::from_slice(&observed.stdout)?;
    let rows = document["rows"]
        .as_array()
        .ok_or("Alignment rows missing")?;
    assert_eq!(document["ranges"], 64, "Original decimal ranges changed");
    assert_eq!(rows.len(), 92, "Alignment observations missing");
    let profile = Profile::from_record(&document["profile"])?;
    for (index, row) in rows.iter().enumerate() {
        let mut seen = document["seen"].clone();
        seen["userAgent"] = row["ua"].clone();
        let before_profile = serde_json::to_value(&profile)?;
        let before_seen = seen.clone();
        let (aligned, changed) = alignment::rewrite(&profile.fingerprint, &seen)?;
        assert_eq!(
            serde_json::to_value(&aligned)?,
            row["fingerprint"],
            "Alignment version fingerprint differs: {index}"
        );
        assert_eq!(
            json!(changed),
            row["changed"],
            "Alignment change labels differ: {index}"
        );
        assert_eq!(serde_json::to_value(&profile)?, before_profile);
        assert_eq!(seen, before_seen);
    }
    temporary.cleanup()?;
    Ok(())
}
