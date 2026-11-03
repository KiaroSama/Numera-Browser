use super::{Integer, registry};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Map, Value, json};
use std::{any::TypeId, io};

fn managed(arg: &Arg) -> bool {
    matches!(arg.get_id().as_str(), "json" | "help")
}
fn multiple(arg: &Arg) -> bool {
    matches!(arg.get_action(), ArgAction::Append)
        || arg
            .get_num_args()
            .is_some_and(|range| range.max_values() > 1)
}
fn scalar(arg: &Arg) -> &'static str {
    let kind = arg.get_value_parser().type_id();
    if kind == TypeId::of::<Integer>() {
        "integer"
    } else if kind == TypeId::of::<f64>() {
        "number"
    } else {
        "string"
    }
}
fn schema(command: &Command) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for arg in command.get_arguments().filter(|arg| !managed(arg)) {
        let name = arg.get_id().as_str();
        let mut property = if matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) {
            json!({"type":"boolean"})
        } else if multiple(arg) {
            json!({"type":"array", "items":{"type":"string"}})
        } else {
            if arg.is_positional() && arg.is_required_set() {
                required.push(name);
            }
            json!({"type":scalar(arg)})
        };
        if !matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse)
            && let Some(values) = arg.get_value_parser().possible_values()
        {
            let values: Vec<_> = values.map(|value| value.get_name().to_owned()).collect();
            if !values.is_empty() {
                property["enum"] = json!(values);
            }
        }
        if let Some(help) = arg
            .get_help()
            .map(ToString::to_string)
            .filter(|s| !s.is_empty())
        {
            property["description"] = help.into();
        }
        properties.insert(name.into(), property);
    }
    properties.insert("detach".into(), json!({"type":"boolean", "description":"Run in the background and return a job handle instead of the result. Use it for anything that opens a browser, then poll with jobs_show."}));
    json!({"type":"object", "properties":properties, "required":required})
}
fn walk<'a>(command: &'a Command, path: Vec<String>, output: &mut Vec<(Vec<String>, &'a Command)>) {
    if !command.has_subcommands() {
        if !path.is_empty() && path != ["serve"] && path != ["mcp"] {
            output.push((path, command));
        }
        return;
    }
    for child in command.get_subcommands() {
        let mut path = path.clone();
        path.push(child.get_name().into());
        walk(child, path, output);
    }
}

/// Derived from the same native parser, never a second hand-maintained tool list.
pub fn tools() -> Vec<Value> {
    let mut command = registry();
    command.build();
    let mut leaves = Vec::new();
    walk(&command, Vec::new(), &mut leaves);
    leaves.into_iter().map(|(path, command)| json!({
        "name":path.join("_").replace('-', "_"),
        "description":command.get_about().map(ToString::to_string).unwrap_or_else(|| format!("numera {}", path.join(" "))),
        "inputSchema":schema(command),
    })).collect()
}

pub fn argv(name: &str, arguments: &Map<String, Value>) -> io::Result<Vec<String>> {
    argv_with_path(name, arguments).map(|(_, argv)| argv)
}

pub(crate) fn argv_with_path(
    name: &str,
    arguments: &Map<String, Value>,
) -> io::Result<(Vec<String>, Vec<String>)> {
    let mut command = registry();
    command.build();
    let mut leaves = Vec::new();
    walk(&command, Vec::new(), &mut leaves);
    let (path, command) = leaves
        .into_iter()
        .find(|(path, _)| path.join("_").replace('-', "_") == name)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no such tool '{name}'")))?;
    let mut positional = Vec::new();
    let mut flags = Vec::new();
    for arg in command.get_arguments().filter(|arg| !managed(arg)) {
        let Some(value) = arguments
            .get(arg.get_id().as_str())
            .filter(|v| !v.is_null())
        else {
            continue;
        };
        let text = |value: &Value| match value {
            Value::Array(_) | Value::Object(_) => crate::validation::repr(value),
            _ => crate::conversion::value_text(value),
        };
        if arg.is_positional() {
            if let Some(values) = value.as_array() {
                positional.extend(values.iter().map(text));
            } else {
                positional.push(text(value));
            }
        } else if let Some(long) = arg.get_long() {
            let flag = format!("--{long}");
            if matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) {
                if crate::conversion::truth(value) {
                    flags.push(flag);
                }
            } else if let Some(values) = value.as_array() {
                for value in values {
                    flags.extend([flag.clone(), text(value)]);
                }
            } else {
                flags.extend([flag, text(value)]);
            }
        }
    }
    if command.get_arguments().all(|arg| arg.get_id() != "detach")
        && arguments
            .get("detach")
            .is_some_and(crate::conversion::truth)
    {
        flags.insert(0, "--detach".into());
    }
    let argv = path
        .iter()
        .cloned()
        .chain(positional)
        .chain(flags)
        .chain(["--json".into()])
        .collect();
    Ok((path, argv))
}

/// Canonical parse view for source comparison and service dispatch.
pub fn values(command: &Command, matches: &ArgMatches) -> Value {
    let mut object = Map::new();
    for arg in command
        .get_arguments()
        .filter(|arg| !matches!(arg.get_id().as_str(), "help" | "version"))
    {
        let id = arg.get_id().as_str();
        let value = if matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) {
            json!(matches.get_flag(id))
        } else if scalar(arg) == "integer" {
            matches
                .get_one::<Integer>(id)
                .and_then(|v| serde_json::from_str(&v.0).ok())
                .unwrap_or(Value::Null)
        } else if scalar(arg) == "number" {
            matches.get_one::<f64>(id).map_or(Value::Null, |v| {
                if matches.value_source(id) == Some(clap::parser::ValueSource::DefaultValue)
                    && let Some(default) = arg.get_default_values().first().and_then(|v| v.to_str())
                    && let Ok(value) = serde_json::from_str::<Value>(default)
                {
                    value
                } else {
                    json!(v)
                }
            })
        } else if multiple(arg) {
            matches.get_many::<String>(id).map_or_else(
                || {
                    if arg.is_positional() {
                        json!([])
                    } else {
                        Value::Null
                    }
                },
                |v| json!(v.collect::<Vec<_>>()),
            )
        } else {
            matches
                .get_one::<String>(id)
                .map_or(Value::Null, |v| json!(v))
        };
        object.insert(id.into(), value);
    }
    if let Some((name, child_matches)) = matches.subcommand()
        && let Some(child) = command.find_subcommand(name)
    {
        let dest = if command.get_name() == "numera" {
            "command".into()
        } else {
            format!("{}_command", command.get_name())
        };
        object.insert(dest, name.into());
        if let Value::Object(child) = values(child, child_matches) {
            object.extend(child);
        }
    }
    Value::Object(object)
}
