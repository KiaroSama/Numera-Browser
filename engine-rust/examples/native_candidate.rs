//! Acceptance-only executable; product selection remains gated by full migration.
fn main() {
    std::process::exit(numera_core::native_entry::run(
        std::env::args().skip(1).collect(),
    ));
}
