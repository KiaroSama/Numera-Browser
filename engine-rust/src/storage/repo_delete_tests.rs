use super::{DeleteRollback, ProfileRepo, RepoError};
use crate::{common::TempDirectory, models::Profile, ownership::capture};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

std::thread_local! {
    static FAULT: RefCell<Option<(usize, PathBuf)>> = const { RefCell::new(None) };
}

pub(super) fn filesystem_fault(path: &Path, restoring: bool) -> io::Result<()> {
    FAULT.with(|fault| {
        let fault = fault.borrow();
        let Some((mode, root)) = fault.as_ref() else {
            return Ok(());
        };
        if !restoring && path == root.join("profiles/fixture-delete.json") {
            if *mode == 3 {
                fs::create_dir(root.join("dashboard/fixture-delete.json"))?;
            }
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "metadata is read-only",
            ));
        }
        if restoring && *mode >= 2 && path.parent() == Some(root.join("user-data").as_path()) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the session directory is held open",
            ));
        }
        Ok(())
    })
}

struct ResetFault;
impl Drop for ResetFault {
    fn drop(&mut self) {
        FAULT.with(|fault| *fault.borrow_mut() = None);
    }
}

#[test]
fn failed_delete_rollback_preserves_causes_and_actual_recovery_paths() -> Result<(), Box<dyn Error>>
{
    let temporary = TempDirectory::new("delete-rollback")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,os,sys
from pathlib import Path
from unittest.mock import patch
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.models import Profile
from numera.store import ProfileStore
from numera.paths import RollbackFailed
from numera.repo import ProfileRepo
from run_logging import run_log
profile=json.loads((repository/'engine-rust/tests/fixtures/repo-delete.json').read_text(encoding='utf-8'))['cases'][0]['profile']
rows=[]
with run_log('delete-rollback-oracle',parent/'logs') as log:
    for mode in range(4):
        root=parent/str(mode);store=ProfileStore(root)
        store.save(Profile.from_dict(profile));store.resolve('fixture-delete')
        session=store.user_data_path('fixture-delete')
        cookie=session/'Cookies';cookie.write_bytes(b'\x00session\xff')
        (root/'config.json').write_bytes(b'{"untouched":true}')
        paths=[root/'dashboard/fixture-delete.json',root/'profiles/fixture-delete.json',root/'config.json']
        before=[p.read_bytes() for p in paths]
        unlink,rename=Path.unlink,Path.rename
        def refuse_unlink(path,*args,**kwargs):
            if mode and path==paths[1]:
                if mode==3:paths[0].mkdir()
                raise PermissionError('metadata is read-only')
            return unlink(path,*args,**kwargs)
        def refuse_rename(path,*args,**kwargs):
            if mode>=2 and path.parent==session.parent and '.deleting-' in path.name:
                raise PermissionError('the session directory is held open')
            return rename(path,*args,**kwargs)
        with patch.object(Path,'unlink',refuse_unlink),patch.object(Path,'rename',refuse_rename):
            try:
                result=ProfileRepo(store).delete('fixture-delete');primary=None;failures=[];message=''
            except OSError as exc:
                result=None;primary=str(exc);failures=[];message=str(exc)
            except RollbackFailed as exc:
                result=None;primary=str(exc.cause);message=str(exc)
                failures=['session' if '.deleting-' in path.name else path.parent.name for path,error in exc.failures]
                assert exc.__cause__ is exc.cause
        tombs=list(session.parent.glob('fixture-delete.deleting-*'))
        recovery=bool(tombs) and str(tombs[0]) in message and str(session) in message
        preserved=[p.is_file() and p.read_bytes()==raw for p,raw in zip(paths,before)]
        state=session if session.exists() else tombs[0] if tombs else None
        intact=state is not None and (state/'Cookies').read_bytes()==b'\x00session\xff'
        assert len(tombs)==int(mode>=2)
        assert recovery==(mode>=2)
        assert failures==(['dashboard','session'] if mode==3 else ['session'] if mode==2 else [])
        rows.append({'result':result,'primary':primary,'failures':failures,'recovery':recovery,
                     'preserved':preserved,'session':session.exists(),'tombs':len(tombs),'intact':intact})
    log.info('Observed delete rollback cases=%s',len(rows))
print(json.dumps(rows))
"#]).arg(repository).arg(temporary.path().join("original"));
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(original.exit, 0, "Original delete rollback observer failed");
    let rows: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(rows.len(), 4);
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/repo-delete.json"))?;
    for (mode, expected) in rows.iter().enumerate() {
        let root = temporary.path().join(format!("native-{mode}"));
        let repo = ProfileRepo::new(root.clone(), None);
        repo.save(
            Profile::from_record(&fixture["cases"][0]["profile"])?,
            1700000000.0,
        )?;
        repo.resolve("fixture-delete", 1700000000.0)?;
        let session = repo.user_data_path("fixture-delete")?;
        fs::write(session.join("Cookies"), b"\x00session\xff")?;
        fs::write(root.join("config.json"), b"{\"untouched\":true}")?;
        let paths = [
            root.join("dashboard/fixture-delete.json"),
            root.join("profiles/fixture-delete.json"),
            root.join("config.json"),
        ];
        let before = paths.iter().map(fs::read).collect::<Result<Vec<_>, _>>()?;
        let reset = ResetFault;
        FAULT.with(|fault| *fault.borrow_mut() = (mode != 0).then(|| (mode, root.clone())));
        let result = repo.delete("fixture-delete", "fixture-host", 1700000000.0);
        drop(reset);
        let tombs = fs::read_dir(root.join("user-data"))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|path| {
                path.file_name().is_some_and(|name| {
                    name.to_string_lossy()
                        .starts_with("fixture-delete.deleting-")
                })
            })
            .collect::<Vec<_>>();
        let mut failure_names = Vec::new();
        let (value, primary, recovery) = match &result {
            Ok(value) => (json!(value), Value::Null, false),
            Err(error) => {
                let message = error.to_string();
                let recovery = tombs.first().is_some_and(|tomb| {
                    message.contains(tomb.to_string_lossy().as_ref())
                        && message.contains(session.to_string_lossy().as_ref())
                });
                assert_eq!(
                    json!(recovery),
                    expected["recovery"],
                    "Delete rollback recovery paths differ: mode={mode}"
                );
                let RepoError::Io(io) = error else {
                    return Err("Delete lost its recoverable I/O error".into());
                };
                let cause = if mode >= 2 {
                    let rollback = io
                        .get_ref()
                        .and_then(|error| error.downcast_ref::<DeleteRollback>())
                        .ok_or("Delete rollback lost its structured failures")?;
                    assert_eq!(
                        rollback.session.as_ref(),
                        tombs
                            .first()
                            .map(|tomb| (tomb.clone(), session.clone()))
                            .as_ref()
                    );
                    for (path, _) in &rollback.failures {
                        failure_names.push(if tombs.contains(path) {
                            "session".to_owned()
                        } else {
                            path.parent()
                                .and_then(Path::file_name)
                                .ok_or("Failure path missing")?
                                .to_string_lossy()
                                .into_owned()
                        });
                        assert!(message.contains(path.to_string_lossy().as_ref()));
                    }
                    assert_eq!(
                        error
                            .source()
                            .and_then(|source| source.downcast_ref::<io::Error>())
                            .map(io::Error::kind),
                        Some(rollback.cause.kind())
                    );
                    &rollback.cause
                } else {
                    io
                };
                (Value::Null, json!(cause.to_string()), recovery)
            }
        };
        let preserved = paths
            .iter()
            .zip(&before)
            .map(|(path, bytes)| {
                path.is_file() && fs::read(path).is_ok_and(|current| current == *bytes)
            })
            .collect::<Vec<_>>();
        let state = if session.is_dir() {
            Some(&session)
        } else {
            tombs.first()
        };
        let intact = state.is_some_and(|path| {
            fs::read(path.join("Cookies")).is_ok_and(|bytes| bytes == b"\x00session\xff")
        });
        assert_eq!(
            json!({"result":value,"primary":primary,"failures":failure_names,"recovery":recovery,
                         "preserved":preserved,"session":session.exists(),"tombs":tombs.len(),"intact":intact}),
            *expected,
            "Delete rollback outcome differs: mode={mode}"
        );
    }
    temporary.cleanup()?;
    Ok(())
}
