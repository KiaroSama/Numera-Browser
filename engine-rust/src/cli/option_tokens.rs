use clap::Command;
use std::ffi::OsString;

pub(super) fn configure(command: Command) -> Command {
    command
        .mut_args(|arg| arg.allow_negative_numbers(false))
        .mut_subcommands(configure)
}

// argparse classifies tokens before conversion, using a different negative-number
// grammar from clap. Bind only original values; keep option errors with clap.
pub(super) fn normalize(command: &Command, arguments: Vec<OsString>) -> Vec<OsString> {
    let mut arguments = arguments.into_iter().peekable();
    let mut output = Vec::new();
    let Some(program) = arguments.next() else {
        return output;
    };
    output.push(program);
    let mut current = command;
    while let Some(argument) = arguments.next() {
        let Some(text) = argument.to_str() else {
            output.push(argument);
            continue;
        };
        if text == "--"
            && current.has_subcommands()
            && arguments
                .peek()
                .and_then(|value| value.to_str())
                .is_some_and(|name| current.find_subcommand(name).is_some())
        {
            // argparse's subparser consumes its own arguments after the marker.
            continue;
        }
        if text == "--" {
            output.push(argument);
            output.extend(arguments);
            break;
        }
        if let Some(name) = text.strip_prefix("--").filter(|name| !name.contains('=')) {
            let exact = current
                .get_arguments()
                .find(|arg| arg.get_long() == Some(name));
            let mut prefixes = current
                .get_arguments()
                .filter(|arg| arg.get_long().is_some_and(|long| long.starts_with(name)));
            let unique = prefixes.next().filter(|_| prefixes.next().is_none());
            if exact.or(unique).is_some_and(|arg| {
                arg.get_num_args()
                    .is_some_and(|range| range.min_values() == 1 && range.max_values() == 1)
            }) {
                if !arguments
                    .peek()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| is_value(current, value))
                {
                    // Do not reinterpret markers or command names after a missing value.
                    output.push(argument);
                    output.extend(arguments);
                    break;
                }
                let value = arguments.next().expect("The option value was inspected");
                let mut attached = argument;
                attached.push("=");
                attached.push(value);
                output.push(attached);
                continue;
            }
        } else if let Some(child) = current.find_subcommand(text) {
            current = child;
        }
        output.push(argument);
    }
    output
}

fn is_value(command: &Command, text: &str) -> bool {
    if !text.starts_with('-') || text == "-" {
        return true;
    }
    if text == "--" {
        return false;
    }
    if let Some(name) = text.strip_prefix("--") {
        let name = name.split('=').next().unwrap_or(name);
        if command
            .get_arguments()
            .any(|arg| arg.get_long().is_some_and(|long| long.starts_with(name)))
        {
            return false;
        }
    } else if let Some(short) = text.chars().nth(1)
        && command
            .get_arguments()
            .any(|arg| arg.get_short() == Some(short))
    {
        return false;
    }
    if text.contains(' ') {
        return true;
    }
    let number = text
        .strip_prefix('-')
        .expect("The token starts with a dash");
    // Python's `$` anchor also matches before one final LF, not before CR.
    let number = number.strip_suffix('\n').unwrap_or(number);
    let decimal = |value: &str| crate::pool::decimal_ascii(value).is_some();
    if let Some((whole, fraction)) = number.split_once('.') {
        !fraction.is_empty() && decimal(whole) && decimal(fraction)
    } else {
        !number.is_empty() && decimal(number)
    }
}
