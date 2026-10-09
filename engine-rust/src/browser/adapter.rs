use crate::ownership::session::Session;
use serde_json::{Map, Value, json};
use std::{
    io::{self, BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::Duration,
};

const MAX_FRAME: usize = 1_048_576;
const PROTOCOL: u64 = 1;
/// The native owner acquires the lease and claim before any SDK operation.
/// Requests carry only effective SDK options, never a source-engine command.
struct PendingWrite {
    bytes: Vec<u8>,
    answer: mpsc::SyncSender<io::Result<()>>,
}
pub struct Adapter {
    session: Session,
    output: Option<Receiver<io::Result<Value>>>,
    input: Option<mpsc::SyncSender<PendingWrite>>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    sequence: u64,
    metadata: Value,
}
fn frame(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut bytes = Vec::new();
    loop {
        let (count, end, eof) = {
            let available = reader.fill_buf()?;
            let eof = available.is_empty();
            let count = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(available.len(), |index| index + 1);
            if bytes.len() + count > MAX_FRAME {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Browser adapter frame exceeds limit",
                ));
            }
            bytes.extend_from_slice(&available[..count]);
            (
                count,
                available.get(count.saturating_sub(1)) == Some(&b'\n'),
                eof,
            )
        };
        reader.consume(count);
        if eof {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Browser adapter ended inside a frame",
                ))
            };
        }
        if end {
            break;
        }
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Browser adapter frame is not UTF8 JSON",
        )
    })?;
    if !value.is_object() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Browser adapter frame is not an object",
        ));
    }
    Ok(Some(value))
}
impl Adapter {
    pub fn spawn(
        root: &Path,
        id: &str,
        host: &str,
        command: &mut Command,
        budget: Duration,
    ) -> io::Result<Self> {
        Self::spawn_admitted(Session::admit(root, id, host)?, command, budget)
    }
    pub fn spawn_admitted(
        mut session: Session,
        command: &mut Command,
        budget: Duration,
    ) -> io::Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env_remove("NUMERA_API_TOKEN")
            .env_remove("NUMERA_PASSWORD")
            .env_remove("NUMERA_TREE")
            .env_remove("NUMERA_READY_FILE");
        session.spawn(command, "browser SDK adapter")?;
        let stdout = session
            .child_mut()?
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("Browser adapter output missing"))?;
        let mut stdin = session
            .child_mut()?
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("Browser adapter input missing"))?;
        let (input, writes) = mpsc::sync_channel::<PendingWrite>(1);
        let writer = thread::spawn(move || {
            while let Ok(PendingWrite { bytes, answer }) = writes.recv() {
                let result = stdin.write_all(&bytes).and_then(|_| stdin.flush());
                let failed = result.is_err();
                let _ = answer.send(result);
                if failed {
                    break;
                }
            }
        });
        let (sender, output) = mpsc::sync_channel(8);
        let reader = thread::spawn(move || {
            let mut input = BufReader::new(stdout);
            loop {
                match frame(&mut input) {
                    Ok(Some(value)) => {
                        if sender.send(Ok(value)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = sender.send(Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "Browser adapter output ended",
                        )));
                        break;
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        });
        let mut adapter = Self {
            session,
            output: Some(output),
            input: Some(input),
            reader: Some(reader),
            writer: Some(writer),
            sequence: 0,
            metadata: Value::Null,
        };
        let hello = adapter.call("hello", Map::new(), budget);
        match hello {
            Ok(metadata) => {
                adapter.metadata = metadata;
                Ok(adapter)
            }
            Err(error) => {
                adapter.finish(budget)?;
                Err(error)
            }
        }
    }
    pub fn metadata(&self) -> &Value {
        &self.metadata
    }
    pub fn open(
        &mut self,
        backend: &str,
        options: Map<String, Value>,
        scripts: Vec<String>,
        budget: Duration,
    ) -> io::Result<()> {
        if !["playwright", "patchright", "camoufox", "cloak"].contains(&backend) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unknown browser backend",
            ));
        }
        if self.metadata["backends"][backend]["installed"] != true {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Requested browser SDK is not installed",
            ));
        }
        let fields = Map::from_iter([
            ("backend".into(), backend.into()),
            ("options".into(), Value::Object(options)),
            ("init_scripts".into(), json!(scripts)),
        ]);
        let value = self.call("open", fields, budget)?;
        if value["opened"] != true || value["backend"] != backend {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Browser adapter did not confirm the requested context",
            ));
        }
        Ok(())
    }
    pub fn call(
        &mut self,
        operation: &str,
        mut fields: Map<String, Value>,
        budget: Duration,
    ) -> io::Result<Value> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Browser adapter request sequence exhausted"))?;
        fields.extend(Map::from_iter([
            ("protocol".into(), PROTOCOL.into()),
            ("id".into(), self.sequence.into()),
            ("operation".into(), operation.into()),
        ]));
        let mut bytes = serde_json::to_vec(&fields).map_err(io::Error::other)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_FRAME {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Browser adapter request exceeds frame limit",
            ));
        }
        let deadline = std::time::Instant::now()
            .checked_add(budget)
            .ok_or_else(|| io::Error::other("Browser adapter request budget is invalid"))?;
        let (done, written) = mpsc::sync_channel(1);
        self.input
            .as_ref()
            .ok_or_else(|| io::Error::other("Browser adapter input is closed"))?
            .try_send(PendingWrite {
                bytes,
                answer: done,
            })
            .map_err(|_| io::Error::other("Browser adapter writer is unavailable"))?;
        written
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Browser adapter request write did not complete",
                )
            })??;
        let value = self
            .output
            .as_ref()
            .ok_or_else(|| io::Error::other("Browser adapter output is closed"))?
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .map_err(|error| {
                io::Error::new(
                    if error == mpsc::RecvTimeoutError::Timeout {
                        io::ErrorKind::TimedOut
                    } else {
                        io::ErrorKind::UnexpectedEof
                    },
                    "Browser adapter did not provide a complete response",
                )
            })??;
        if value["protocol"].as_u64() != Some(PROTOCOL)
            || value["id"].as_u64() != Some(self.sequence)
            || !value["ok"].is_boolean()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Browser adapter response envelope is invalid",
            ));
        }
        if value["ok"] != true {
            return Err(io::Error::other(
                "Browser SDK operation failed; inspect the sanitized adapter log",
            ));
        }
        value.get("result").cloned().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Browser adapter result missing")
        })
    }
    pub fn finish(&mut self, budget: Duration) -> io::Result<()> {
        self.input.take();
        self.output.take();
        self.session.finish(budget)?;
        if let Some(writer) = self.writer.take() {
            writer
                .join()
                .map_err(|_| io::Error::other("Browser adapter input writer failed"))?;
        }
        if let Some(reader) = self.reader.take() {
            reader
                .join()
                .map_err(|_| io::Error::other("Browser adapter output reader failed"))?;
        }
        Ok(())
    }
}
impl Drop for Adapter {
    fn drop(&mut self) {
        if let Err(error) = self.finish(Duration::from_secs(2)) {
            eprintln!("Browser adapter cleanup remains unverified: {error}");
        }
    }
}
