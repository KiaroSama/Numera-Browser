#![windows_subsystem = "windows"]

use std::{
    env, io,
    os::windows::process::CommandExt,
    process::{self, Command, Stdio},
};

fn run() -> io::Result<i32> {
    let mut args = env::args_os().skip(1);
    let Some(program) = args.next() else {
        return Ok(0);
    };
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    // No breakaway flag: the real fixture stays in the launcher's owned Job.
    let status = Command::new(program)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .creation_flags(DETACHED_PROCESS)
        .status()?;
    status
        .code()
        .ok_or_else(|| io::Error::other("Fixture child exit code unavailable"))
}

fn main() {
    match run() {
        Ok(code) => process::exit(code),
        Err(error) => {
            eprintln!("Could not run the detached fixture: {error}");
            process::exit(1);
        }
    }
}
