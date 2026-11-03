//! Source-compatible protocol dispatch; executable child supervision is separate.
use crate::{
    cli::{profile_commands::Outcome, reflection},
    conversion::truth,
};
use serde_json::{Map, Value, json};
use std::io;

pub const CALL_TIMEOUT_SECONDS: u64 = 120;
pub const MAX_FRAME: usize = 1_048_576;
pub const DEFAULT_PROTOCOL: &str = "2024-11-05";

fn ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}
pub fn tool_result(outcome: Outcome) -> io::Result<Value> {
    Ok(
        json!({"content":[{"type":"text","text":source_text(&outcome.payload)?}],"isError":false,
        "structuredContent":if outcome.payload.is_object(){outcome.payload.clone()}else{json!({"result":outcome.payload})},"_meta":{"exitCode":outcome.exit}}),
    )
}
pub fn execute_native(
    application: &crate::cli::application::Application<'_>,
    argv: Vec<String>,
    now: f64,
) -> io::Result<Outcome> {
    let mut registry = crate::cli::registry();
    registry.build();
    let matches = crate::cli::parse(std::iter::once("numera".to_owned()).chain(argv))
        .map_err(io::Error::other)?;
    let values = reflection::values(&registry, &matches);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(CALL_TIMEOUT_SECONDS);
    let mut timed_out = false;
    let result = application.execute_controlled(
        values
            .as_object()
            .ok_or_else(|| io::Error::other("Native CLI values missing"))?,
        now,
        |_| {
            Err(crate::storage::repo::RepoError::Message(
                "this needs confirmation — pass -y (a --json run never prompts)".into(),
            ))
        },
        || {
            timed_out = std::time::Instant::now() >= deadline;
            timed_out
        },
    );
    if timed_out {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Native MCP command exceeded its deadline; call it with detach: true",
        ));
    }
    match result {
        Ok(outcome) => Ok(outcome),
        Err(error) => Ok(Outcome::failed(error.to_string())),
    }
}
pub(crate) fn error_result(message: String) -> io::Result<Value> {
    error_payload(json!({"error":message}))
}
pub(crate) fn error_payload(payload: Value) -> io::Result<Value> {
    Ok(json!({"content":[{"type":"text","text":source_text(&payload)?}],"isError":true}))
}

pub fn handle(
    message: &Value,
    mut execute: impl FnMut(Vec<String>) -> io::Result<Outcome>,
) -> io::Result<Option<Value>> {
    handle_tools(message, |name, arguments| {
        match reflection::argv(name, arguments) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                error_result(format!("no such tool '{name}'"))
            }
            Err(error) => Err(error),
            Ok(argv) => match execute(argv) {
                Ok(outcome) => tool_result(outcome),
                Err(error) => error_result(error.to_string()),
            },
        }
    })
}

pub(crate) fn handle_tools<E: From<io::Error>>(
    message: &Value,
    mut execute: impl FnMut(&str, &Map<String, Value>) -> Result<Value, E>,
) -> Result<Option<Value>, E> {
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let empty = Map::new();
    let params = message
        .get("params")
        .filter(|v| truth(v))
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let result = match message.get("method").and_then(Value::as_str) {
        Some("initialize") => ok(
            id,
            json!({"protocolVersion":params.get("protocolVersion").filter(|v|truth(v)).cloned().unwrap_or_else(||DEFAULT_PROTOCOL.into()),
            "capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"numera","version":env!("CARGO_PKG_VERSION")}}),
        ),
        Some("tools/list") => ok(id, json!({"tools":reflection::tools()})),
        Some("tools/call") => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = params
                .get("arguments")
                .filter(|v| truth(v))
                .and_then(Value::as_object)
                .unwrap_or(&empty);
            let result = execute(name, arguments)?;
            ok(id, result)
        }
        Some("ping") => ok(id, json!({})),
        method => {
            if message.get("id").is_none_or(Value::is_null) {
                return Ok(None);
            }
            let method = method.unwrap_or("None");
            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":format!("unknown method '{method}'")}})
        }
    };
    Ok(Some(result))
}

/// Decode one bounded UTF-8 line. Malformed JSON is ignored as in the source loop.
pub fn decode(bytes: &[u8]) -> io::Result<Option<Value>> {
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MCP frame exceeds limit",
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "MCP frame is not UTF8"))?;
    let text = crate::pool::trim(text);
    if text.is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<Value>(text) {
        Ok(message) if message.is_object() => Ok(Some(message)),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MCP message is not an object",
        )),
        Err(_) => Ok(None),
    }
}
pub fn encode(message: &Value) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(message).map_err(io::Error::other)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MCP response exceeds limit",
        ));
    }
    Ok(bytes)
}

/// Sequential stdio transport. Tool failures retain their caller-owned error type.
pub fn serve<E: From<io::Error>>(
    input: &mut impl io::BufRead,
    output: &mut impl io::Write,
    mut dispatch: impl FnMut(&Value) -> Result<Option<Value>, E>,
) -> Result<(), E> {
    let mut frame = Vec::new();
    loop {
        let available = match input.fill_buf() {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        let eof = available.is_empty();
        let count = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if count > MAX_FRAME.saturating_sub(frame.len()) {
            return Err(
                io::Error::new(io::ErrorKind::InvalidData, "MCP frame exceeds limit").into(),
            );
        }
        frame.extend_from_slice(&available[..count]);
        let complete = frame.last() == Some(&b'\n');
        input.consume(count);
        if (complete || eof) && !frame.is_empty() {
            if let Some(message) = decode(&frame)?
                && let Some(response) = dispatch(&message)?
            {
                output.write_all(&encode(&response)?)?;
                output.flush()?;
            }
            frame.clear();
        }
        if eof {
            return Ok(());
        }
    }
}

fn source_text(value: &Value) -> io::Result<String> {
    use serde::Serialize;
    let mut bytes = Vec::new();
    value
        .serialize(&mut serde_json::Serializer::with_formatter(
            &mut bytes,
            crate::identity_script::SourceJson,
        ))
        .map_err(io::Error::other)?;
    String::from_utf8(bytes).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn stdio_propagates_output_failure_without_dispatching_the_next_frame() {
        let input = b"{\"id\":1,\"method\":\"ping\"}\n{\"id\":2,\"method\":\"ping\"}\n";
        let mut calls = 0;
        let mut output: &mut [u8] = &mut [];
        let error = serve(&mut Cursor::new(input), &mut output, |message| {
            calls += 1;
            handle(message, |_| panic!("Ping executed a tool"))
        })
        .expect_err("Failed output was ignored");
        assert_eq!(error.kind(), io::ErrorKind::WriteZero);
        assert_eq!(calls, 1);
    }

    #[test]
    fn stdio_bounds_frames_before_dispatch_and_preserves_source_eof() -> io::Result<()> {
        let input =
            b"not JSON\n \r\n{\"id\":1,\"method\":\"ping\"}\r\n{\"id\":2,\"method\":\"ping\"}";
        let mut reader = BufReader::with_capacity(3, Cursor::new(input));
        let mut output = Vec::new();
        serve(&mut reader, &mut output, |message| {
            handle(message, |_| panic!("Ping executed a tool"))
        })?;
        assert_eq!(output, b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}\n");
        for input in [vec![b'x'; MAX_FRAME + 1], vec![0xff, b'\n']] {
            let mut reader = BufReader::with_capacity(7, Cursor::new(input));
            let error = serve::<io::Error>(&mut reader, &mut Vec::new(), |_| {
                panic!("Invalid frame reached dispatch")
            })
            .expect_err("Invalid frame accepted");
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        }
        let mut boundary = vec![b' '; MAX_FRAME - 1];
        boundary.push(b'\n');
        serve::<io::Error>(&mut Cursor::new(boundary), &mut Vec::new(), |_| {
            panic!("Whitespace reached dispatch")
        })?;
        Ok(())
    }
}
