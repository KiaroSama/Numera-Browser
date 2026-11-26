fn main() -> std::process::ExitCode {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--container-smoke")) {
        return numera_core::browser::sdk::container_smoke::main();
    }
    numera_core::browser::sdk::main()
}
