//! Native product entry, built explicitly with its browser worker and dashboard.
fn main() {
    let _installation = match std::env::current_exe().and_then(|exe| {
        numera_core::installation_gate::RuntimeLease::for_executable(
            &exe,
            std::env::var("NUMERA_UPDATE_ACTIVATION").ok().as_deref(),
        )
    }) {
        Ok(lease) => lease,
        Err(_) => {
            eprintln!("Recovery owns this installation");
            std::process::exit(1);
        }
    };
    if std::env::args().skip(1).eq(["--verify-installation"]) {
        let result =
            std::env::current_exe().and_then(|exe| {
                numera_core::installation::verify(exe.parent().ok_or_else(|| {
                    std::io::Error::other("Native installation directory is missing")
                })?)
            });
        if result.is_err() {
            eprintln!("Native installation verification failed");
            std::process::exit(1);
        }
        return;
    }
    if std::env::args().skip(1).eq(["--http-healthcheck"]) {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .and_then(|runtime| {
                runtime.block_on(numera_core::installation::health(
                    "http://127.0.0.1:8787/api/health",
                    &std::env::var("NUMERA_API_TOKEN").unwrap_or_default(),
                ))
            });
        if result.is_err() {
            eprintln!("Local Numera server is not healthy");
            std::process::exit(1);
        }
        return;
    }
    if std::env::args().skip(1).eq(["--build-info"]) {
        match numera_core::native_build::identity() {
            Ok(identity) => println!("{identity}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    std::process::exit(numera_core::native_entry::run(
        std::env::args().skip(1).collect(),
    ));
}
