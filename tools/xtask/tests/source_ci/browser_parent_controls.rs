fn parent_publication(publication: &str) -> TestResult {
    // This is the real post-OS-spawn/pre-receipt hook. The observer must not
    // mistake its parent's still-pending transfer for a missing identity.
    let temp = Temp::new("observer parent publication")?;
    let source = r#"
// Both the observer and its application receive explicit stdio pipes from
// their owners. A Windows console is unnecessary and can add a conhost to
// the very Job whose natural exit the fixture must prove.
#![cfg_attr(windows, windows_subsystem = "windows")]
use numera_xtask::ci::{self,browser_benchmark::{Native,observe}};
use numera_core::ownership::{test_support::{self,Backend},unit_report};
use serde_json::json;
use std::{fs,io::{self,Write},path::{Path,PathBuf},process::Command,sync::Arc,time::{Duration,Instant}};
struct Delay { root:PathBuf, mode:String }
impl Backend for Delay {
 fn spawned(&self,record:&serde_json::Value)->Option<io::Result<()>> {
  Some((|| {
   let report=PathBuf::from(std::env::var_os(unit_report::ENV).ok_or_else(||io::Error::other("Parent receipt missing"))?);
   let lock=numera_core::storage::locks::DataLock::for_file(&report)?;
   let held=if self.mode=="held-lock" {Some(lock.acquire(Duration::from_secs(1))?)}else{None};
   let original=if matches!(self.mode.as_str(),"malformed"|"wrong-birth") {
    let _guard=lock.acquire(Duration::from_secs(1))?;
    let original=fs::read(&report)?;
    if self.mode=="malformed" {
     fs::OpenOptions::new().append(true).open(&report)?.write_all(b"{broken receipt\n")?;
    }
    Some(original)
   }else{None};
   if self.mode=="wrong-birth" {
    let pending=String::from_utf8(original.as_ref().ok_or_else(||io::Error::other("Original receipt missing"))?.clone()).map_err(io::Error::other)?;
    let attempt=pending.lines().filter_map(|line|serde_json::from_str::<serde_json::Value>(line).ok()).filter(|row|row["event"]=="pending").last().and_then(|row|row["attempt"].as_str().map(str::to_owned)).ok_or_else(||io::Error::other("Actual pending attempt missing"))?;
    let mut wrong=record.clone();
    wrong["token"]=(record["token"].as_u64().ok_or_else(||io::Error::other("Actual birth missing"))?+1).into();
    unit_report::record_at(&report,&wrong,&attempt)?;
   }
   let deadline=Instant::now()+Duration::from_secs(3);
   while !self.root.join("observer-started").is_file() {
    if Instant::now()>=deadline {return Err(io::Error::other("Observer did not enter registration"));}
    std::thread::sleep(Duration::from_millis(1));
   }
   std::thread::sleep(Duration::from_millis(80));
   if self.mode!="delayed" {
    let deadline=Instant::now()+Duration::from_millis(400);
    while !self.root.join("observer-refused").is_file() && Instant::now()<deadline {
     std::thread::sleep(Duration::from_millis(1));
    }
   }
   if self.root.join("observer-refused").is_file() {
    fs::write(self.root.join("refused-before-publication"),b"actual observer refused while publication held")?;
   }
   drop(held);
   if let Some(original)=original {
    let _guard=lock.acquire(Duration::from_secs(1))?;
    fs::write(&report,original)?;
   }
   Ok(())
  })())
 }
}
fn main()->ci::Result<()> {
 let args=std::env::args_os().skip(1).collect::<Vec<_>>();
 let mode=args[0].to_str().ok_or("Mode missing")?;
 let root=Path::new(&args[1]);
 let publication=args[2].to_str().ok_or("Publication mode missing")?;
 if mode=="app" {
  fs::write(root.join("application-started"),b"actual application executed")?;
  println!("{{\"ok\":true}}");return Ok(());
 }
 if mode=="observer" {
  fs::write(root.join("observer-started"),b"actual observer entered")?;
  let mut command=Command::new(std::env::current_exe()?);
  command.arg("app").arg(root).arg(publication);
  let report=std::env::var_os(unit_report::ENV).ok_or("Parent receipt missing")?;
  let mut native=Native::new(command,report.into(),false)?;
  let wall=if publication=="delayed" {Duration::from_secs(2)}else{Duration::from_millis(30)};
  let result=observe(&mut native,wall);
  match result {
   Ok(sample)=>println!("{}",json!({"sample":sample.summary(),"payload":sample.payload})),
   Err(error)=>{fs::write(root.join("observer-refused"),b"refused before application spawn")?;println!("{}",json!({"refused":true}));eprintln!("{error}");std::process::exit(1);}
  }
  return Ok(());
 }
 let mut command=Command::new(std::env::current_exe()?);
 command.arg("observer").arg(root).arg(publication);
 let output=test_support::with_backend(Arc::new(Delay{root:root.into(),mode:publication.into()}),||
  ci::owned::run(root,&mut command,ci::owned::limits(Duration::from_secs(5),Duration::from_secs(5),16384)))?.command;
 println!("{}",json!({"exit":output.exit,"stdout":String::from_utf8(output.stdout)?,"stderr":String::from_utf8(output.stderr)?}));
 Ok(())
}
"#;
    let executable = super::migration::compile(&temp, source)?;
    fs::create_dir(temp.0.join("jobs"))?;
    let report = temp.0.join("jobs/registration.units");
    numera_core::ownership::unit_report::initialize(&report)?;
    let output = ci::capture(
        std::process::Command::new(executable)
            .arg("parent")
            .arg(&temp.0)
            .arg(publication)
            .env(numera_core::ownership::unit_report::ENV, &report),
        Duration::from_secs(8),
        Duration::from_secs(8),
        32768,
    )?;
    assert_eq!(
        output.exit,
        0,
        "Parent publication helper failed ({publication})\nstdout:\n{}\nstderr:\n{}",
        numera_core::ownership::resources::redact_diagnostics(&String::from_utf8_lossy(
            &output.stdout
        )),
        numera_core::ownership::resources::redact_diagnostics(&String::from_utf8_lossy(
            &output.stderr
        )),
    );
    let answer = ci::decode(&output.stdout)?;
    if publication == "delayed" && answer["exit"] != 0 {
        assert!(
            answer["stderr"].as_str()
                == Some("Browser observer receipt registration or birth identity missing\n"),
            "Observer refused outside the expected parent receipt boundary"
        );
        assert!(!temp.0.join("application-started").exists());
    }
    let observed = ci::decode(
        answer["stdout"]
            .as_str()
            .ok_or("Observer stdout missing")?
            .as_bytes(),
    )?;
    if publication == "delayed" {
        assert_eq!(
            answer["exit"], 0,
            "Observer refused a genuine delayed parent publication"
        );
        assert!(temp.0.join("application-started").is_file());
        assert_eq!(observed["payload"], json!({"ok":true}));
        assert!(answer["stderr"].as_str().is_some_and(str::is_empty));
    } else {
        assert_eq!(answer["exit"], 1);
        assert_eq!(observed["refused"], true);
        assert!(!temp.0.join("application-started").exists());
        assert!(temp.0.join("refused-before-publication").is_file());
    }
    Ok(())
}
