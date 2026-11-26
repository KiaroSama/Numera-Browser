//! Acceptance-only executable; product selection remains gated by full migration.
fn main() {
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
