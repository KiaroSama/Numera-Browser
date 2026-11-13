//! Native inert producer for bounded process and resource boundary tests.
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
    time::Duration,
};
fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("emit") => {
            if let Some(path) = args.get(1) {
                fs::write(
                    path,
                    std::env::var("NUMERA_HELPER_UNITS").unwrap_or_default(),
                )?;
            }
            print!("Fixture α");
            Ok(7)
        }
        Some("json") => {
            if let Some(path) = args.get(1) {
                fs::write(
                    path,
                    std::env::var("NUMERA_HELPER_UNITS").unwrap_or_default(),
                )?;
            }
            println!("{{}}");
            Ok(0)
        }
        Some("replay-output") => {
            io::stdout().write_all(&fs::read(args.get(1).ok_or("output path missing")?)?)?;
            Ok(args.get(2).ok_or("exit code missing")?.parse()?)
        }
        Some("input") => {
            let mut data = Vec::new();
            io::stdin().read_to_end(&mut data)?;
            if data != b"y\n" {
                return Err("stdin byte mismatch".into());
            }
            print!("Fixture α");
            Ok(7)
        }
        Some("invalid") => {
            io::stdout().write_all(&[0xff])?;
            Ok(7)
        }
        Some("hold") => {
            if let Some(path) = args.get(1) {
                fs::write(path, std::process::id().to_string())?;
            }
            std::thread::sleep(Duration::from_secs(30));
            Ok(0)
        }
        Some("resource-worker") => {
            use numera_core::ownership::resources::{ResourceFailure, ResourceGuard};
            let root = Path::new(args.get(1).ok_or("root missing")?);
            let leaked = args.get(2).is_some_and(|s| s == "unsettled");
            let guard = ResourceGuard::new()?;
            let mut file = Some(fs::File::open(root.join("payload"))?);
            let (send, receive) = std::sync::mpsc::sync_channel::<()>(1);
            let worker = std::thread::spawn(move || {
                let _ = receive.recv();
            });
            if !leaked {
                drop(file.take());
                send.send(())?;
                worker.join().map_err(|_| "resource fixture panicked")?;
                let e = guard.finish()?;
                println!(
                    "{{\"ok\":true,\"evidence\":{{\"before\":{{\"handles\":{},\"threads\":{}}},\"after\":{{\"handles\":{},\"threads\":{}}},\"panics\":{}}}}}",
                    e.before.handles, e.before.threads, e.after.handles, e.after.threads, e.panics
                );
            } else {
                let e = match guard.finish() {
                    Err(ResourceFailure::Residual(e)) => e,
                    _ => return Err("resource leak was not observed".into()),
                };
                println!(
                    "{{\"ok\":false,\"evidence\":{{\"before\":{{\"handles\":{},\"threads\":{}}},\"after\":{{\"handles\":{},\"threads\":{}}},\"panics\":{}}}}}",
                    e.before.handles, e.before.threads, e.after.handles, e.after.threads, e.panics
                );
                drop(file);
                send.send(())?;
                worker.join().map_err(|_| "resource fixture panicked")?;
            }
            Ok(0)
        }
        _ => Err("Unknown native acceptance fixture mode".into()),
    }
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1)
        }
    }
}
