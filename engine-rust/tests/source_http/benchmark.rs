use super::*;

use numera_core::benchmark_http::{self, Memory, Observation, Sample};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener},
    thread,
    time::Duration,
};
struct Scripted {
    mode: &'static str,
    clock: u64,
    calls: Vec<&'static str>,
    closed: bool,
}
impl Scripted {
    fn new(mode: &'static str) -> Self {
        Self {
            mode,
            clock: 0,
            calls: vec![],
            closed: false,
        }
    }
}
impl Observation for Scripted {
    fn spawn(&mut self) -> io::Result<()> {
        self.calls.push("spawn");
        Ok(())
    }
    fn receipt(&mut self) -> io::Result<()> {
        self.calls.push("record");
        if self.mode == "receipt" {
            Err(io::Error::other("receipt failed"))
        } else {
            Ok(())
        }
    }
    fn exited(&mut self) -> io::Result<bool> {
        Ok(self.mode == "early-exit")
    }
    fn ready(&mut self) -> io::Result<()> {
        self.calls.push("/api/ready");
        self.clock += 1000;
        Ok(())
    }
    fn ticks(&mut self) -> u64 {
        self.clock
    }
    fn request(&mut self) -> io::Result<Value> {
        self.calls.push("/api/profiles");
        self.clock += 7;
        if self.mode == "request" {
            Err(io::Error::other("invalid HTTP response"))
        } else {
            Ok(json!([{"id":"fixture"}]))
        }
    }
    fn settle(&mut self) -> io::Result<()> {
        self.calls.push("settle");
        if self.mode == "settlement" {
            Err(io::Error::other("tree did not settle"))
        } else {
            Ok(())
        }
    }
    fn reap(&mut self) -> io::Result<()> {
        self.calls.push("wait");
        if self.mode == "wait" {
            Err(io::Error::new(io::ErrorKind::TimedOut, "reap failed"))
        } else {
            Ok(())
        }
    }
    fn close_pipes(&mut self) -> io::Result<()> {
        self.closed = true;
        Ok(())
    }
    fn peak(&mut self) -> io::Result<Memory> {
        self.calls.push("peak");
        Ok(Memory {
            metric: "peak-child-rss-bytes".into(),
            bytes: 1234,
        })
    }
}
fn wire_request(mode: &str) -> TestResult {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let payload = match mode {
        "utf8" => vec![0xff],
        "json" => b"{".to_vec(),
        "size" => vec![b'x'; 1_048_577],
        _ => br#"[{"id":"fixture"}]"#.to_vec(),
    };
    let status = if mode == "status" { 503 } else { 200 };
    let content_type = if mode == "content-type" {
        "text/plain"
    } else {
        "application/json"
    };
    let (framing, body) = if mode == "chunked" {
        (
            String::from("Transfer-Encoding: chunked\r\n"),
            [
                format!("{:x}\r\n", payload.len()).into_bytes(),
                payload.clone(),
                b"\r\n0\r\n\r\n".to_vec(),
            ]
            .concat(),
        )
    } else if mode == "eof" {
        (String::from("Connection: close\r\n"), payload.clone())
    } else {
        (
            format!(
                "Content-Length: {}\r\n",
                payload.len() + if mode == "truncated" { 7 } else { 0 }
            ),
            payload.clone(),
        )
    };
    let wire = [
        format!("HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\n{framing}\r\n")
            .into_bytes(),
        body,
    ]
    .concat();
    let server = thread::spawn(move || -> io::Result<bool> {
        let (mut socket, _) = listener.accept()?;
        socket.set_read_timeout(Some(Duration::from_secs(3)))?;
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte)?;
            request.push(byte[0]);
            if request.len() > 8192 {
                return Err(io::Error::other("request bound"));
            }
        }
        let text = String::from_utf8_lossy(&request).to_lowercase();
        assert!(text.starts_with("get /api/profiles http/1.1"));
        assert!(text.contains("authorization: bearer benchmark-only"));
        let _ = socket.write_all(&wire);
        let _ = socket.shutdown(Shutdown::Write);
        let mut byte = [0];
        match socket.read(&mut byte) {
            Ok(0) => Ok(true),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
                ) =>
            {
                Ok(true)
            }
            other => Err(io::Error::other(format!(
                "connection not closed: {other:?}"
            ))),
        }
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(benchmark_http::request(
        &format!("http://127.0.0.1:{port}/api/profiles"),
        "benchmark-only",
        Duration::from_secs(2),
    ));
    if ["normal", "chunked", "eof"].contains(&mode) {
        assert_eq!(result?, json!([{"id":"fixture"}]));
    } else {
        assert!(result.is_err(), "{mode} invalid response accepted");
    }
    drop(runtime);
    assert!(server.join().map_err(|_| "HTTP wire fixture panicked")??);
    Ok(())
}
source_test!(
    http_benchmark_test_http_observer_bootstraps_under_isolated_python_from_an_unrelated_directory,
    "tests/test_http_benchmark.py::test_http_observer_bootstraps_under_isolated_python_from_an_unrelated_directory",
    {
        let t = fresh();
        let output = Command::new(env!("CARGO_BIN_EXE_numera-http-observer"))
            .arg("--help")
            .current_dir(t.path())
            .output()?;
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout)?;
        assert!(help.contains("--runtime"));
        assert!(help.contains("--data-dir"));
        assert!(output.stderr.is_empty());
        #[cfg(any(target_os = "linux", windows))]
        {
            let root = t.path().join("data");
            fs::create_dir(&root)?;
            fs::write(root.join(".imported"), "isolated observer\n")?;
            let store = ProfileRepo::new(root.clone(), None);
            store.save(profile("Observed"), 1000.0)?;
            let receipt = t.path().join("observation.receipt");
            let measured = Command::new(env!("CARGO_BIN_EXE_numera-http-observer"))
                .args([
                    "--runtime",
                    "native",
                    "--candidate",
                    env!("CARGO_BIN_EXE_numera-engine"),
                    "--data-dir",
                ])
                .arg(&root)
                .arg("--receipt")
                .arg(&receipt)
                .env("NUMERA_LOG_DIR", t.path().join("logs"))
                .current_dir(t.path())
                .output()?;
            assert!(
                measured.status.success(),
                "{}",
                String::from_utf8_lossy(&measured.stderr)
            );
            let sample: Sample = serde_json::from_slice(&measured.stdout)?;
            assert_eq!(sample.payload[0]["name"], "Observed");
            assert!(sample.elapsed_ns > 0);
            assert!(sample.memory.bytes > 0);
            assert!(receipt.is_file());
        }
        Ok(())
    }
);
source_test!(http_benchmark_test_http_sample_measures_only_request_and_rejects_failed_ownership_normal_3c8d8c,"tests/test_http_benchmark.py::test_http_sample_measures_only_request_and_rejects_failed_ownership[normal]",{
let mut host=Scripted::new("normal");let result=benchmark_http::observe(&mut host);let sample=result?;assert_eq!(sample.elapsed_ns,7);assert_eq!(sample.memory.metric,"peak-child-rss-bytes");assert_eq!(sample.memory.bytes,1234);assert_eq!(sample.payload,json!([{"id":"fixture"}]));let peak=host.calls.iter().position(|c|*c=="peak").expect("peak");let wait=host.calls.iter().position(|c|*c=="wait").expect("wait");let request=host.calls.iter().position(|c|*c=="/api/profiles").expect("request");assert!(peak>wait&&wait>request);assert_eq!(&host.calls[..2],&["spawn","record"]);assert_eq!(host.calls.iter().filter(|c|**c=="settle").count(),1);assert_eq!(host.calls.iter().filter(|c|**c=="wait").count(),1);assert!(host.closed);
Ok(())
});
source_test!(http_benchmark_test_http_sample_measures_only_request_and_rejects_failed_ownership_request_50b02e,"tests/test_http_benchmark.py::test_http_sample_measures_only_request_and_rejects_failed_ownership[request]",{
let mut host=Scripted::new("request");let result=benchmark_http::observe(&mut host);assert!(result.is_err());assert!(!host.calls.contains(&"peak"));assert_eq!(&host.calls[..2],&["spawn","record"]);assert_eq!(host.calls.iter().filter(|c|**c=="settle").count(),1);assert_eq!(host.calls.iter().filter(|c|**c=="wait").count(),1);assert!(host.closed);
Ok(())
});
source_test!(http_benchmark_test_http_sample_measures_only_request_and_rejects_failed_ownership_early_exit_2694b4,"tests/test_http_benchmark.py::test_http_sample_measures_only_request_and_rejects_failed_ownership[early-exit]",{
let mut host=Scripted::new("early-exit");let result=benchmark_http::observe(&mut host);assert!(result.is_err());assert!(!host.calls.contains(&"peak"));assert_eq!(&host.calls[..2],&["spawn","record"]);assert_eq!(host.calls.iter().filter(|c|**c=="settle").count(),1);assert_eq!(host.calls.iter().filter(|c|**c=="wait").count(),1);assert!(host.closed);
Ok(())
});
source_test!(http_benchmark_test_http_sample_measures_only_request_and_rejects_failed_ownership_receipt_9987d5,"tests/test_http_benchmark.py::test_http_sample_measures_only_request_and_rejects_failed_ownership[receipt]",{
let mut host=Scripted::new("receipt");let result=benchmark_http::observe(&mut host);assert!(result.is_err());assert!(!host.calls.contains(&"peak"));assert_eq!(&host.calls[..2],&["spawn","record"]);assert_eq!(host.calls.iter().filter(|c|**c=="settle").count(),1);assert_eq!(host.calls.iter().filter(|c|**c=="wait").count(),1);assert!(host.closed);
Ok(())
});
source_test!(http_benchmark_test_http_sample_measures_only_request_and_rejects_failed_ownership_settlement_2f7da6,"tests/test_http_benchmark.py::test_http_sample_measures_only_request_and_rejects_failed_ownership[settlement]",{
let mut host=Scripted::new("settlement");let result=benchmark_http::observe(&mut host);assert!(result.is_err());assert!(!host.calls.contains(&"peak"));assert_eq!(&host.calls[..2],&["spawn","record"]);assert_eq!(host.calls.iter().filter(|c|**c=="settle").count(),1);assert_eq!(host.calls.iter().filter(|c|**c=="wait").count(),1);assert!(host.closed);
Ok(())
});
source_test!(
    http_benchmark_test_http_sample_measures_only_request_and_rejects_failed_ownership_wait_01ebcc,
    "tests/test_http_benchmark.py::test_http_sample_measures_only_request_and_rejects_failed_ownership[wait]",
    {
        let mut host = Scripted::new("wait");
        let result = benchmark_http::observe(&mut host);
        assert!(result.is_err());
        assert!(!host.calls.contains(&"peak"));
        assert_eq!(&host.calls[..2], &["spawn", "record"]);
        assert_eq!(host.calls.iter().filter(|c| **c == "settle").count(), 1);
        assert_eq!(host.calls.iter().filter(|c| **c == "wait").count(), 1);
        assert!(host.closed);
        Ok(())
    }
);
source_test!(http_benchmark_test_http_request_requires_bounded_complete_json_and_closes_connection_normal_4cd01e,"tests/test_http_benchmark.py::test_http_request_requires_bounded_complete_json_and_closes_connection[normal]",{
wire_request("normal")?;
Ok(())
});
source_test!(http_benchmark_test_http_request_requires_bounded_complete_json_and_closes_connection_chunked_684770,"tests/test_http_benchmark.py::test_http_request_requires_bounded_complete_json_and_closes_connection[chunked]",{
wire_request("chunked")?;
Ok(())
});
source_test!(http_benchmark_test_http_request_requires_bounded_complete_json_and_closes_connection_eof_da27d2,"tests/test_http_benchmark.py::test_http_request_requires_bounded_complete_json_and_closes_connection[eof]",{
wire_request("eof")?;
Ok(())
});
source_test!(http_benchmark_test_http_request_requires_bounded_complete_json_and_closes_connection_status_148408,"tests/test_http_benchmark.py::test_http_request_requires_bounded_complete_json_and_closes_connection[status]",{
wire_request("status")?;
Ok(())
});
source_test!(http_benchmark_test_http_request_requires_bounded_complete_json_and_closes_connection_content_type_b8dc4a,"tests/test_http_benchmark.py::test_http_request_requires_bounded_complete_json_and_closes_connection[content-type]",{
wire_request("content-type")?;
Ok(())
});
source_test!(http_benchmark_test_http_request_requires_bounded_complete_json_and_closes_connection_size_0d631d,"tests/test_http_benchmark.py::test_http_request_requires_bounded_complete_json_and_closes_connection[size]",{
wire_request("size")?;
Ok(())
});
source_test!(http_benchmark_test_http_request_requires_bounded_complete_json_and_closes_connection_utf8_116e38,"tests/test_http_benchmark.py::test_http_request_requires_bounded_complete_json_and_closes_connection[utf8]",{
wire_request("utf8")?;
Ok(())
});
source_test!(http_benchmark_test_http_request_requires_bounded_complete_json_and_closes_connection_json_d4852b,"tests/test_http_benchmark.py::test_http_request_requires_bounded_complete_json_and_closes_connection[json]",{
wire_request("json")?;
Ok(())
});
source_test!(http_benchmark_test_http_request_requires_bounded_complete_json_and_closes_connection_truncated_db8b26,"tests/test_http_benchmark.py::test_http_request_requires_bounded_complete_json_and_closes_connection[truncated]",{
wire_request("truncated")?;
Ok(())
});
source_test!(
    http_benchmark_test_http_pair_preserves_complete_payload_and_input_bytes_normal_b45c76,
    "tests/test_http_benchmark.py::test_http_pair_preserves_complete_payload_and_input_bytes[normal]",
    {
        let t = fresh();
        let root = t.path();
        fs::write(root.join("config.json"), br#"{"future":true}"#)?;
        let rows=(0..32).map(|i|json!({"id":format!("{i:012x}"),"name":format!("Fixture {i}"),"future":[true,7]})).collect::<Vec<_>>();
        let protected =
            benchmark_http::PairEvidence::capture(root, rows.iter().map(|p| text(p, "id")))?;
        let changed = rows.clone();
        let result = protected.validate(Sample {
            elapsed_ns: 7,
            memory: Memory {
                metric: "fixture".into(),
                bytes: 1234,
            },
            payload: json!(changed),
        });
        let (measurement, payload) = result?;
        assert_eq!(measurement["elapsed_ns"], 7);
        assert_eq!(payload, json!(changed));
        assert_eq!(payload, json!(rows));
        Ok(())
    }
);
source_test!(
    http_benchmark_test_http_pair_preserves_complete_payload_and_input_bytes_missing_697452,
    "tests/test_http_benchmark.py::test_http_pair_preserves_complete_payload_and_input_bytes[missing]",
    {
        let t = fresh();
        let root = t.path();
        fs::write(root.join("config.json"), br#"{"future":true}"#)?;
        let rows=(0..32).map(|i|json!({"id":format!("{i:012x}"),"name":format!("Fixture {i}"),"future":[true,7]})).collect::<Vec<_>>();
        let protected =
            benchmark_http::PairEvidence::capture(root, rows.iter().map(|p| text(p, "id")))?;
        let mut changed = rows.clone();
        changed.pop();
        let result = protected.validate(Sample {
            elapsed_ns: 7,
            memory: Memory {
                metric: "fixture".into(),
                bytes: 1234,
            },
            payload: json!(changed),
        });
        assert!(result.is_err());
        Ok(())
    }
);
source_test!(
    http_benchmark_test_http_pair_preserves_complete_payload_and_input_bytes_duplicate_7f572b,
    "tests/test_http_benchmark.py::test_http_pair_preserves_complete_payload_and_input_bytes[duplicate]",
    {
        let t = fresh();
        let root = t.path();
        fs::write(root.join("config.json"), br#"{"future":true}"#)?;
        let rows=(0..32).map(|i|json!({"id":format!("{i:012x}"),"name":format!("Fixture {i}"),"future":[true,7]})).collect::<Vec<_>>();
        let protected =
            benchmark_http::PairEvidence::capture(root, rows.iter().map(|p| text(p, "id")))?;
        let mut changed = rows.clone();
        changed[0] = changed[1].clone();
        let result = protected.validate(Sample {
            elapsed_ns: 7,
            memory: Memory {
                metric: "fixture".into(),
                bytes: 1234,
            },
            payload: json!(changed),
        });
        assert!(result.is_err());
        Ok(())
    }
);
source_test!(
    http_benchmark_test_http_pair_preserves_complete_payload_and_input_bytes_identity_6ef50d,
    "tests/test_http_benchmark.py::test_http_pair_preserves_complete_payload_and_input_bytes[identity]",
    {
        let t = fresh();
        let root = t.path();
        fs::write(root.join("config.json"), br#"{"future":true}"#)?;
        let rows=(0..32).map(|i|json!({"id":format!("{i:012x}"),"name":format!("Fixture {i}"),"future":[true,7]})).collect::<Vec<_>>();
        let protected =
            benchmark_http::PairEvidence::capture(root, rows.iter().map(|p| text(p, "id")))?;
        let mut changed = rows.clone();
        changed[0]["id"] = json!([]);
        let result = protected.validate(Sample {
            elapsed_ns: 7,
            memory: Memory {
                metric: "fixture".into(),
                bytes: 1234,
            },
            payload: json!(changed),
        });
        assert!(result.is_err());
        Ok(())
    }
);
source_test!(
    http_benchmark_test_http_pair_preserves_complete_payload_and_input_bytes_changed_bytes_067b4e,
    "tests/test_http_benchmark.py::test_http_pair_preserves_complete_payload_and_input_bytes[changed-bytes]",
    {
        let t = fresh();
        let root = t.path();
        fs::write(root.join("config.json"), br#"{"future":true}"#)?;
        let rows=(0..32).map(|i|json!({"id":format!("{i:012x}"),"name":format!("Fixture {i}"),"future":[true,7]})).collect::<Vec<_>>();
        let protected =
            benchmark_http::PairEvidence::capture(root, rows.iter().map(|p| text(p, "id")))?;
        let changed = rows.clone();
        fs::write(root.join("config.json"), br#"{"future":false}"#)?;
        let result = protected.validate(Sample {
            elapsed_ns: 7,
            memory: Memory {
                metric: "fixture".into(),
                bytes: 1234,
            },
            payload: json!(changed),
        });
        assert!(result.is_err());
        Ok(())
    }
);
source_test!(
    http_benchmark_test_http_pair_preserves_complete_payload_and_input_bytes_changed_field_f30481,
    "tests/test_http_benchmark.py::test_http_pair_preserves_complete_payload_and_input_bytes[changed-field]",
    {
        let t = fresh();
        let root = t.path();
        fs::write(root.join("config.json"), br#"{"future":true}"#)?;
        let rows=(0..32).map(|i|json!({"id":format!("{i:012x}"),"name":format!("Fixture {i}"),"future":[true,7]})).collect::<Vec<_>>();
        let protected =
            benchmark_http::PairEvidence::capture(root, rows.iter().map(|p| text(p, "id")))?;
        let mut changed = rows.clone();
        changed[0]["future"] = json!([1, 7]);
        let result = protected.validate(Sample {
            elapsed_ns: 7,
            memory: Memory {
                metric: "fixture".into(),
                bytes: 1234,
            },
            payload: json!(changed),
        });
        let (measurement, payload) = result?;
        assert_eq!(measurement["elapsed_ns"], 7);
        assert_eq!(payload, json!(changed));
        assert_ne!(payload, json!(rows));
        Ok(())
    }
);
source_test!(
    http_benchmark_test_http_pair_preserves_complete_payload_and_input_bytes_reordered_b15d9d,
    "tests/test_http_benchmark.py::test_http_pair_preserves_complete_payload_and_input_bytes[reordered]",
    {
        let t = fresh();
        let root = t.path();
        fs::write(root.join("config.json"), br#"{"future":true}"#)?;
        let rows=(0..32).map(|i|json!({"id":format!("{i:012x}"),"name":format!("Fixture {i}"),"future":[true,7]})).collect::<Vec<_>>();
        let protected =
            benchmark_http::PairEvidence::capture(root, rows.iter().map(|p| text(p, "id")))?;
        let mut changed = rows.clone();
        changed.reverse();
        let result = protected.validate(Sample {
            elapsed_ns: 7,
            memory: Memory {
                metric: "fixture".into(),
                bytes: 1234,
            },
            payload: json!(changed),
        });
        let (measurement, payload) = result?;
        assert_eq!(measurement["elapsed_ns"], 7);
        assert_eq!(payload, json!(changed));
        assert_ne!(payload, json!(rows));
        Ok(())
    }
);
source_test!(
    http_benchmark_test_complete_http_benchmark_retains_failed_pairs_until_acceptance_normal_de0678,
    "tests/test_http_benchmark.py::test_complete_http_benchmark_retains_failed_pairs_until_acceptance[normal]",
    {
        let t = fresh();
        let parent = t.path().join("retained");
        let destination = t.path().join("measurements.json");
        let mut observed = Vec::new();
        let result = benchmark_http::run_pairs(&parent, &destination, |runtime, directory| {
            fs::write(directory.join("input.json"), r#"{"synthetic":true}"#)?;
            observed.push(runtime.to_owned());
            let metric = if runtime == "native" && true { 1 } else { 100 };
            Ok(Sample {
                elapsed_ns: metric,
                memory: Memory {
                    metric: "fixture".into(),
                    bytes: metric,
                },
                payload: json!("same full ordered response"),
            })
        });
        assert!(result.is_ok());
        assert_eq!(fs::read_dir(&parent)?.count(), 0);
        let evidence = read(&destination);
        assert_eq!(evidence["warmup_pairs"], 1);
        assert_eq!(
            evidence["results"][0]["samples"]
                .as_array()
                .expect("measured samples")
                .len(),
            7
        );
        let expected = (-1i32..7)
            .flat_map(|i| {
                if i % 2 == 0 {
                    ["original", "native"]
                } else {
                    ["native", "original"]
                }
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(observed, expected);
        Ok(())
    }
);
source_test!(
    http_benchmark_test_complete_http_benchmark_retains_failed_pairs_until_acceptance_parity_305557,
    "tests/test_http_benchmark.py::test_complete_http_benchmark_retains_failed_pairs_until_acceptance[parity]",
    {
        let t = fresh();
        let parent = t.path().join("retained");
        let destination = t.path().join("measurements.json");
        let mut observed = Vec::new();
        let result = benchmark_http::run_pairs(&parent, &destination, |runtime, directory| {
            fs::write(directory.join("input.json"), r#"{"synthetic":true}"#)?;
            observed.push(runtime.to_owned());
            let metric = if runtime == "native" && true { 1 } else { 100 };
            Ok(Sample {
                elapsed_ns: metric,
                memory: Memory {
                    metric: "fixture".into(),
                    bytes: metric,
                },
                payload: json!(runtime),
            })
        });
        assert!(result.is_err());
        let roots = fs::read_dir(&parent)?
            .map(|e| e.map(|e| e.path()))
            .collect::<io::Result<Vec<_>>>()?;
        assert_eq!(roots.len(), 1);
        let inputs = fs::read_dir(&roots[0])?
            .map(|e| e.map(|e| e.path().join("input.json")))
            .collect::<io::Result<Vec<_>>>()?;
        assert_eq!(
            inputs.iter().filter(|p| p.is_file()).count(),
            observed.len()
        );
        Ok(())
    }
);
source_test!(http_benchmark_test_complete_http_benchmark_retains_failed_pairs_until_acceptance_inconclusive_4a2b59,"tests/test_http_benchmark.py::test_complete_http_benchmark_retains_failed_pairs_until_acceptance[inconclusive]",{
let t=fresh();let parent=t.path().join("retained");let destination=t.path().join("measurements.json");let mut observed=Vec::new();let result=benchmark_http::run_pairs(&parent,&destination,|runtime,directory|{fs::write(directory.join("input.json"),r#"{"synthetic":true}"#)?;observed.push(runtime.to_owned());let metric=if runtime=="native"&&false{1}else{100};Ok(Sample{elapsed_ns:metric,memory:Memory{metric:"fixture".into(),bytes:metric},payload:json!("same full ordered response")})});assert!(result.is_err());let roots=fs::read_dir(&parent)?.map(|e|e.map(|e|e.path())).collect::<io::Result<Vec<_>>>()?;assert_eq!(roots.len(),1);let inputs=fs::read_dir(&roots[0])?.map(|e|e.map(|e|e.path().join("input.json"))).collect::<io::Result<Vec<_>>>()?;assert_eq!(inputs.iter().filter(|p|p.is_file()).count(),observed.len());let evidence=read(&destination);assert_eq!(evidence["warmup_pairs"],1);assert_eq!(evidence["results"][0]["samples"].as_array().expect("measured samples").len(),7);let expected=(-1i32..7).flat_map(|i|if i%2==0{["original","native"]}else{["native","original"]}).map(str::to_owned).collect::<Vec<_>>();assert_eq!(observed,expected);
Ok(())
});
source_test!(
    http_benchmark_test_complete_http_benchmark_retains_failed_pairs_until_acceptance_write_731e14,
    "tests/test_http_benchmark.py::test_complete_http_benchmark_retains_failed_pairs_until_acceptance[write]",
    {
        let t = fresh();
        let parent = t.path().join("retained");
        let destination = t.path().join("measurements.json");
        fs::write(&destination, "keep original evidence")?;
        let mut observed = Vec::new();
        let result = benchmark_http::run_pairs(&parent, &destination, |runtime, directory| {
            fs::write(directory.join("input.json"), r#"{"synthetic":true}"#)?;
            observed.push(runtime.to_owned());
            let metric = if runtime == "native" && true { 1 } else { 100 };
            Ok(Sample {
                elapsed_ns: metric,
                memory: Memory {
                    metric: "fixture".into(),
                    bytes: metric,
                },
                payload: json!("same full ordered response"),
            })
        });
        assert!(result.is_err());
        let roots = fs::read_dir(&parent)?
            .map(|e| e.map(|e| e.path()))
            .collect::<io::Result<Vec<_>>>()?;
        assert_eq!(roots.len(), 1);
        let inputs = fs::read_dir(&roots[0])?
            .map(|e| e.map(|e| e.path().join("input.json")))
            .collect::<io::Result<Vec<_>>>()?;
        assert_eq!(
            inputs.iter().filter(|p| p.is_file()).count(),
            observed.len()
        );
        assert_eq!(fs::read_to_string(&destination)?, "keep original evidence");
        let expected = (-1i32..7)
            .flat_map(|i| {
                if i % 2 == 0 {
                    ["original", "native"]
                } else {
                    ["native", "original"]
                }
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(observed, expected);
        Ok(())
    }
);
