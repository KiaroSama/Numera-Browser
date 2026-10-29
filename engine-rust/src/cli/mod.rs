//! Native command definitions shared by CLI parsing and automation reflection.
//! Command execution remains separate; this registry never forwards the Python app.
mod administration;
pub mod align_commands;
pub mod application;
pub mod attach_commands;
pub mod automation_output;
mod browser;
mod bulk_commands;
pub mod cookie_commands;
pub mod cookie_output;
mod definitions;
pub mod extension_commands;
pub mod extension_output;
mod fleet_commands;
pub mod fleet_output;
mod import_commands;
pub mod job_commands;
pub mod job_render;
pub mod launch_commands;
mod option_tokens;
pub mod output;
pub mod pool_commands;
pub mod pool_output;
mod presentation;
pub mod probe_commands;
pub mod probe_output;
pub mod profile_commands;
pub mod profile_output;
mod profiles;
pub mod reflection;
pub mod schedule_commands;
pub mod settings_commands;
pub mod settings_output;
pub mod vault_commands;
pub mod warmup_commands;

use clap::{Arg, Command};
pub use definitions::Integer;
use definitions::{command, option};

pub fn registry() -> Command {
    let mut root = command("numera", "Browser fingerprint & profile manager.")
        .version(env!("CARGO_PKG_VERSION"))
        .disable_help_subcommand(true)
        .subcommand_required(true)
        .arg(option(
            "data_dir",
            "data-dir",
            "Override the data directory (default: ~/.numera)",
        ))
        .arg(option(
            "password",
            "password",
            "Master password for the secrets vault (or set the NUMERA_PASSWORD env var)",
        ));
    // Preserve the source command ordering without a second automation registry.
    for command in profiles::primary() {
        root = root.subcommand(command);
    }
    for command in browser::commands() {
        root = root.subcommand(command);
    }
    for command in administration::commands() {
        root = root.subcommand(command);
    }
    root
}

pub fn parse<I, T>(arguments: I) -> Result<clap::ArgMatches, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut command = option_tokens::configure(registry());
    command.build();
    let arguments =
        option_tokens::normalize(&command, arguments.into_iter().map(Into::into).collect());
    command.try_get_matches_from(arguments)
}

pub(crate) fn agent(command: Command) -> Command {
    command
        .arg(
            Arg::new("json")
                .long("json")
                .action(clap::ArgAction::SetTrue)
                .help("Print the result as JSON on stdout (and nothing else)"),
        )
        .arg(
            Arg::new("detach")
                .long("detach")
                .action(clap::ArgAction::SetTrue)
                .help("Run in the background and print a job handle instead of blocking"),
        )
}
