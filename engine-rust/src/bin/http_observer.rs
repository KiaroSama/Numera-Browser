fn main() -> std::process::ExitCode {
    match numera_core::benchmark_http::main() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("HTTP observation failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
