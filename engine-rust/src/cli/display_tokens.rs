use clap::{Arg, ArgAction, Command};
use std::ffi::OsString;

// Called only after an unknown-argument error. argparse defers unknown options but
// executes displays before final extra/required-argument checks. Earlier value
// errors still win; this path never returns executable ArgMatches.
pub(super) fn after_unknown(command: &Command, arguments: &[OsString]) -> Option<clap::Error> {
    let mut current = command;
    let mut index = 1;
    while index < arguments.len() {
        // Only ASCII option names are classified here. Unknown native bytes
        // remain extras; this projection never becomes an accepted value.
        let text = arguments[index].to_string_lossy();
        if text == "--" {
            return None;
        }
        let value_like = super::option_tokens::is_value(current, &text);
        if !value_like && let Some(name) = text.strip_prefix("--") {
            let (name, attached) = name
                .split_once('=')
                .map_or((name, None), |(name, value)| (name, Some(value)));
            if let Some(arg) = super::option_tokens::long_option(current, name) {
                if takes_value(arg) {
                    let native;
                    let value = if let Some(value) = attached {
                        value
                    } else {
                        index += 1;
                        let Some(argument) = arguments.get(index) else {
                            return valid_value(current, arg, None).err();
                        };
                        native = argument.to_string_lossy();
                        if !super::option_tokens::is_value(current, &native) {
                            return valid_value(current, arg, None).err();
                        }
                        native.as_ref()
                    };
                    if let Err(error) = valid_value(current, arg, Some(value)) {
                        return Some(error);
                    }
                } else {
                    if attached.is_some() {
                        return syntax_error(current, text.as_ref());
                    }
                    if let Some(display) = display(current, arg.get_action(), false) {
                        return Some(display);
                    }
                }
            } else if current
                .get_arguments()
                .filter(|arg| arg.get_long().is_some_and(|long| long.starts_with(name)))
                .take(2)
                .count()
                > 1
            {
                return syntax_error(current, text.as_ref());
            }
        } else if text.starts_with('-') && !value_like {
            let mut queued = None;
            let shorts = &text[1..];
            for (offset, short) in shorts.char_indices() {
                let Some(arg) = current
                    .get_arguments()
                    .find(|arg| arg.get_short() == Some(short))
                else {
                    break;
                };
                if takes_value(arg) {
                    return None;
                }
                let rest = &shorts[offset + short.len_utf8()..];
                if rest.starts_with(['=', '-']) {
                    let long = arg.get_long()?;
                    let value = rest.strip_prefix('=').unwrap_or(rest);
                    return syntax_error(current, &format!("--{long}={value}"));
                }
                if queued.is_none() {
                    queued = display(current, arg.get_action(), true);
                }
            }
            if queued.is_some() {
                return queued;
            }
        } else if current.has_subcommands() {
            // A subcommand selector is positional, not a deferrable extra.
            current = current.find_subcommand(text.as_ref())?;
        }
        index += 1;
    }
    None
}

fn takes_value(arg: &Arg) -> bool {
    arg.get_num_args()
        .is_none_or(|range| range.max_values() > 0)
}

fn valid_value(command: &Command, arg: &Arg, value: Option<&str>) -> Result<(), clap::Error> {
    // Reuse actual integer/float/choice validation without requiring unrelated
    // command arguments. Its diagnostic takes precedence over deferred extras.
    let long = arg.get_long().expect("A long option was selected");
    Command::new(command.get_name().to_owned())
        .bin_name(command.get_bin_name().unwrap_or(command.get_name()))
        .disable_help_flag(true)
        .arg(arg.clone().required(false))
        .try_get_matches_from([
            OsString::from("value-check"),
            value
                .map_or_else(|| format!("--{long}"), |value| format!("--{long}={value}"))
                .into(),
        ])
        .map(|_| ())
}

fn syntax_error(command: &Command, argument: &str) -> Option<clap::Error> {
    command
        .clone()
        .try_get_matches_from(["numera", argument])
        .err()
        .filter(|error| error.exit_code() != 0)
}

fn display(command: &Command, action: &ArgAction, short: bool) -> Option<clap::Error> {
    let flag = match action {
        ArgAction::Version => "--version",
        ArgAction::Help if short => "-h",
        ArgAction::HelpShort => "-h",
        ArgAction::Help | ArgAction::HelpLong => "--help",
        _ => return None,
    };
    let mut command = command.clone();
    command
        .try_get_matches_from_mut(["numera", flag])
        .err()
        .filter(|error| error.exit_code() == 0)
}
