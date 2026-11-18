//! Process-only controls for the historical native entry and MCP boundaries.
use serde_json::Value;
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
};

pub fn dispatch(config: &Value, args: &[OsString]) -> io::Result<bool> {
    if config["reject"] == true {
        return Err(io::Error::other(
            "Store-only command activated the SDK fixture",
        ));
    }
    if args.first().is_some_and(|arg| arg == "--metadata") && config["metadata_failure"] == true {
        return Err(io::Error::other("Deliberate native metadata refusal"));
    }
    if config["mode"] != "mcp-output" {
        return Ok(false);
    }
    if args.len() != 4 || args[0] != "list" || args[1] != "--tag" || args[3] != "--json" {
        return Err(io::Error::other("MCP command argv differs"));
    }
    let index = args[2]
        .to_str()
        .and_then(|text| text.parse::<usize>().ok())
        .ok_or_else(|| io::Error::other("MCP output row missing"))?;
    let rows: Value = serde_json::from_slice(&fs::read(
        config["cases"]
            .as_str()
            .ok_or_else(|| io::Error::other("MCP output corpus missing"))?,
    )?)?;
    let row = rows
        .as_array()
        .and_then(|rows| rows.get(index))
        .ok_or_else(|| io::Error::other("MCP output index invalid"))?;
    io::stdout().write_all(
        row["stdout"]
            .as_str()
            .ok_or_else(|| io::Error::other("MCP stdout missing"))?
            .as_bytes(),
    )?;
    io::stderr().write_all(
        row["stderr"]
            .as_str()
            .ok_or_else(|| io::Error::other("MCP stderr missing"))?
            .as_bytes(),
    )?;
    io::stdout().flush()?;
    io::stderr().flush()?;
    let exit = i32::try_from(
        row["exit"]
            .as_i64()
            .ok_or_else(|| io::Error::other("MCP exit missing"))?,
    )
    .map_err(io::Error::other)?;
    std::process::exit(exit)
}
