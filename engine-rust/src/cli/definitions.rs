use clap::{Arg, ArgAction, Command};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Integer(pub String);
fn integer(text: &str) -> Result<Integer, String> {
    let value =
        crate::decimal::integer(text).ok_or_else(|| format!("invalid int value: {text:?}"))?;
    let negative = value.starts_with('-');
    let digits = value.trim_start_matches('-').trim_start_matches('0');
    Ok(Integer(if digits.is_empty() {
        "0".into()
    } else if negative {
        format!("-{digits}")
    } else {
        digits.into()
    }))
}

pub(super) fn command(name: &'static str, help: &'static str) -> Command {
    Command::new(name)
        .about(help)
        .args_override_self(true)
        .infer_long_args(true)
        .disable_help_subcommand(true)
}
pub(super) fn leaf(name: &'static str, help: &'static str) -> Command {
    super::agent(command(name, help))
}
pub(super) fn group(name: &'static str, help: &'static str, commands: Vec<Command>) -> Command {
    super::agent(command(name, help))
        .subcommand_required(true)
        .subcommands(commands)
}
pub(super) fn positional(name: &'static str, help: &'static str) -> Arg {
    Arg::new(name).required(true).help(help)
}
pub(super) fn option(name: &'static str, flag: &'static str, help: &'static str) -> Arg {
    Arg::new(name).long(flag).help(help)
}
pub(super) fn flag(name: &'static str, long: &'static str, help: &'static str) -> Arg {
    option(name, long, help).action(ArgAction::SetTrue)
}
pub(super) fn repeated(name: &'static str, long: &'static str, help: &'static str) -> Arg {
    option(name, long, help).action(ArgAction::Append)
}
pub(super) fn whole(name: &'static str, long: &'static str, help: &'static str) -> Arg {
    option(name, long, help)
        .value_parser(integer)
        .allow_negative_numbers(true)
}
pub(super) fn real(name: &'static str, long: &'static str, help: &'static str) -> Arg {
    option(name, long, help)
        .allow_negative_numbers(true)
        .value_parser(|text: &str| {
            crate::browser::cookie_expiry::numeric_text(text)
                .ok_or_else(|| format!("invalid float value: {text:?}"))
        })
}
pub(super) fn profile() -> Arg {
    positional("ref", "")
}
pub(super) fn engine(help: &'static str) -> Arg {
    option("engine", "engine", help)
}
pub(super) fn os() -> Arg {
    option("os", "os", "").value_parser(["windows", "macos", "linux", "android"])
}
pub(super) fn preset(help: &'static str) -> Arg {
    option("preset", "preset", help).value_parser([
        "general",
        "ads",
        "ecommerce",
        "crypto",
        "social",
        "news",
    ])
}
