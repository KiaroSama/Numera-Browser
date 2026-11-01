use clap::{Arg, Command};
use std::ffi::OsString;

// Keep original value bytes in ArgMatches. Only already-valid leaf allocations
// move past a marker; argparse consumes a variadic positional in its first run.
pub(super) fn normalize(command: &Command, arguments: Vec<OsString>) -> Vec<OsString> {
    let mut current = command;
    let mut start = 1;
    while current.has_subcommands() && start < arguments.len() {
        let Some(text) = arguments[start].to_str() else {
            return arguments;
        };
        if let Some(child) = current.find_subcommand(text) {
            current = child;
        } else if !option(current, text) {
            return arguments;
        }
        start += 1;
    }
    if current.has_subcommands() {
        return arguments;
    }
    let mut positionals = current.get_positionals().collect::<Vec<_>>();
    positionals.sort_by_key(|arg| arg.get_index());
    if positionals.is_empty()
        || positionals.len() > 1 && positionals.iter().any(|arg| range(arg).1 > 1)
    {
        return arguments;
    }
    let mut flags = Vec::new();
    let mut values = Vec::new();
    let (mut escaped, mut needed, mut run, mut next) = (false, false, 0, 0);
    for argument in &arguments[start..] {
        let Some(text) = argument.to_str() else {
            return arguments;
        };
        if !escaped && text == "--" {
            escaped = true;
        } else if escaped || super::option_tokens::is_value(current, text) {
            needed |= !escaped && text.starts_with('-') && text != "-";
            values.push(argument.clone());
            run += 1;
        } else {
            if !option(current, text) || !allocate(&positionals, &mut next, run) {
                return arguments;
            }
            run = 0;
            flags.push(argument.clone());
        }
    }
    if !needed
        || !allocate(&positionals, &mut next, run)
        || positionals[next..].iter().any(|arg| range(arg).0 > 0)
    {
        return arguments;
    }
    let mut output = arguments[..start].to_vec();
    output.extend(flags);
    output.push("--".into());
    output.extend(values);
    output
}

fn range(arg: &Arg) -> (usize, usize) {
    arg.get_num_args()
        .map(|range| (range.min_values(), range.max_values()))
        .unwrap_or((1, 1))
}

fn allocate(positionals: &[&Arg], next: &mut usize, mut count: usize) -> bool {
    while count > 0 {
        let Some(arg) = positionals.get(*next) else {
            return false;
        };
        let (minimum, maximum) = range(arg);
        if count < minimum {
            return false;
        }
        count -= count.min(maximum);
        *next += 1;
    }
    true
}

// Separate single-value options have already been bound by option_tokens.
// Leave unknown, ambiguous, missing-value and short-value grammar with clap.
fn option(command: &Command, text: &str) -> bool {
    if let Some(long) = text.strip_prefix("--") {
        let (name, value) = long
            .split_once('=')
            .map_or((long, false), |(name, _)| (name, true));
        return super::option_tokens::long_option(command, name).is_some_and(|arg| {
            let (minimum, maximum) = range(arg);
            if value {
                minimum == 1 && maximum == 1
            } else {
                maximum == 0
            }
        });
    }
    text.strip_prefix('-').is_some_and(|shorts| {
        !shorts.is_empty()
            && shorts.chars().all(|short| {
                command
                    .get_arguments()
                    .find(|arg| arg.get_short() == Some(short))
                    .is_some_and(|arg| range(arg).1 == 0)
            })
    })
}
