use super::*;

pub(super) fn exact_inventory_controls() -> TestResult {
    browser_compiler_controls()?;
    let baseline = ci::decode(&fs::read(root().join("docs/rust-test-baseline.json"))?)?;
    let mut paths = fs::read_dir(root().join("docs"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.retain(|path| {
        path.file_name()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.starts_with("source-") && v.ends_with("-test-map.json"))
    });
    paths.sort();
    let maps = paths
        .iter()
        .map(|path| ci::decode(&fs::read(path)?))
        .collect::<ci::Result<Vec<_>>>()?;
    ci::coverage_inventory::validate_documents(&root(), &baseline, &maps)?;
    let selected = maps
        .iter()
        .position(|map| map["suite"] == "source_ci")
        .ok_or("CI source map missing")?;
    for damage in ["missing", "duplicate", "kind", "platform"] {
        let mut changed = maps.clone();
        let map = &mut changed[selected];
        match damage {
            "missing" => {
                let removed = map["cases"].as_array_mut().expect("cases").remove(0);
                let count = map["source_count"].as_u64().expect("count") - 1;
                map["source_count"] = json!(count);
                map["implemented_count"] = json!(count);
                let kind = removed["original_type"].as_str().expect("kind");
                let count = map["case_types"][kind].as_u64().expect("kind count") - 1;
                map["case_types"][kind] = json!(count);
            }
            "duplicate" => map["cases"][1] = map["cases"][0].clone(),
            "kind" => map["cases"][0]["original_type"] = json!("standard"),
            _ => map["cases"][0]["native_platforms"] = json!(["windows"]),
        }
        assert!(
            ci::coverage_inventory::validate_documents(&root(), &baseline, &changed).is_err(),
            "{damage}"
        );
    }
    let expected = BTreeSet::from(["exact_original_case".into()]);
    let valid = "running 1 test\ntest exact_original_case ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n";
    assert_eq!(
        ci::coverage_inventory::validate_expected_output(&expected, valid, 0)?,
        1
    );
    for damaged in [
        valid.replace("test exact_original_case ... ok\n", ""),
        valid.replace("exact_original_case", "invented_replacement"),
        valid.replace(" ... ok", " ... ignored"),
        valid.replace("running 1 test", "running 0 tests"),
    ] {
        assert!(ci::coverage_inventory::validate_expected_output(&expected, &damaged, 0).is_err());
    }
    Ok(())
}
fn browser_compiler_controls() -> TestResult {
    let native = ci::workflow::parse(&fs::read_to_string(
        root().join(".github/workflows/native-ci.yml"),
    )?)?;
    let advisory = ci::workflow::parse(&fs::read_to_string(
        root().join(".github/workflows/dependency-review.yml"),
    )?)?;
    ci::workflow::browser_page_compiler(&native, &advisory)?;
    let candidate = ci::workflow::parse(&fs::read_to_string(
        root().join(".github/workflows/rust-candidate.yml"),
    )?)?;
    for document in [&native, &candidate] {
        ci::workflow::branch_concurrency(document)?;
        for damage in ["event", "repository", "disabled"] {
            let mut changed = document.clone();
            match damage {
                "event" => {
                    changed["concurrency"]["group"] =
                        json!("${{ github.workflow }}-${{ github.event_name }}-${{ github.ref }}")
                }
                "repository" => {
                    changed["concurrency"]["group"] =
                        json!("${{ github.workflow }}-${{ github.head_ref || github.ref_name }}")
                }
                _ => changed["concurrency"]["cancel-in-progress"] = json!(false),
            }
            assert!(
                ci::workflow::branch_concurrency(&changed).is_err(),
                "{damage}"
            );
        }
    }
    let dependencies =
        ci::workflow::parse(&fs::read_to_string(root().join(".github/dependabot.yml"))?)?;
    ci::workflow::dependency_updates(&dependencies)?;
    for damage in ["missing", "duplicate", "pip", "npm"] {
        let mut changed = dependencies.clone();
        let updates = changed["updates"].as_array_mut().expect("updates");
        match damage {
            "missing" => {
                updates.pop();
            }
            "duplicate" => updates.push(updates[0].clone()),
            other => updates[0]["package-ecosystem"] = json!(other),
        }
        assert!(
            ci::workflow::dependency_updates(&changed).is_err(),
            "{damage}"
        );
    }
    let check = ci::workflow::step_once(
        &native["jobs"]["static"]["steps"],
        "--bin browser-page-build -- --check",
        true,
    )?;
    for damage in [
        "missing",
        "conditional",
        "softened",
        "stale",
        "target",
        "archive",
        "binary",
    ] {
        let mut changed = native.clone();
        let lane = &mut changed["jobs"]["static"];
        match damage {
            "missing" => {
                lane["steps"].as_array_mut().expect("steps").remove(check);
            }
            "conditional" => lane["steps"][check]["if"] = json!("false"),
            "softened" => lane["steps"][check]["continue-on-error"] = json!(true),
            "stale" => {
                lane["steps"][check]["run"] = json!(
                    lane["steps"][check]["run"]
                        .as_str()
                        .expect("run")
                        .replace("--check", "--release")
                )
            }
            "target" => lane["steps"][1]["with"]["targets"] = json!("x86_64-unknown-linux-gnu"),
            _ => {
                let setup = ci::workflow::step_once(
                    &lane["steps"],
                    "binaryen-version_133-x86_64-linux.tar.gz",
                    true,
                )?;
                let pin = if damage == "archive" {
                    "2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489"
                } else {
                    "52597efec0ad2416f8b5c245864960e775b8ffc94c751cc0b016c4b4986d3bcb"
                };
                lane["steps"][setup]["run"] = json!(
                    lane["steps"][setup]["run"]
                        .as_str()
                        .expect("run")
                        .replace(pin, &"0".repeat(64))
                );
            }
        }
        assert!(
            ci::workflow::browser_page_compiler(&changed, &advisory).is_err(),
            "{damage}"
        );
    }
    let mut missing = advisory.clone();
    let steps = missing["jobs"]["audit"]["steps"]
        .as_array_mut()
        .expect("audit steps");
    for step in steps {
        if let Some(run) = step["run"].as_str() {
            step["run"] = json!(
                run.lines()
                    .filter(|line| !line.contains("browser-page-rust/Cargo.lock"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
    assert!(ci::workflow::browser_page_compiler(&native, &missing).is_err());
    Ok(())
}

struct Natural {
    clock: u64,
    mode: &'static str,
    events: Vec<&'static str>,
    queries: usize,
}
impl browser::Observer for Natural {
    fn register(&mut self) -> TestResult {
        Ok(())
    }
    fn ticks(&mut self) -> u64 {
        self.clock
    }
    fn spawn(&mut self) -> TestResult {
        self.events.push("spawn");
        Ok(())
    }
    fn communicate(&mut self, _: u64) -> ci::Result<Value> {
        self.clock = if self.mode == "expired" {
            15_000_000_000
        } else {
            0
        };
        Ok(json!({"ok":true}))
    }
    fn natural(&mut self, _: u64) -> ci::Result<bool> {
        self.queries += 1;
        if self.mode == "late" {
            self.clock = 15_000_000_000;
            return Ok(true);
        }
        if self.mode == "unknown" {
            return Ok(false);
        }
        Ok(self.mode == "transient" && self.clock >= 250_000_000)
    }
    fn pause(&mut self) -> TestResult {
        self.clock += 10_000_000;
        Ok(())
    }
    fn peak(&mut self) -> ci::Result<(String, u64)> {
        self.events.push("peak");
        Ok(("peak-job-committed-bytes".into(), 1234))
    }
    fn cleanup(&mut self) -> TestResult {
        self.events.push("cleanup");
        Ok(())
    }
    fn reap(&mut self) -> TestResult {
        self.events.push("reap");
        Ok(())
    }
    fn close(&mut self) -> TestResult {
        self.events
            .extend(["stdout-close", "stderr-close", "process-close", "job-close"]);
        Ok(())
    }
}

pub(super) fn natural_deadline() -> TestResult {
    historical_metadata(
        "migration::migration_natural_deadline",
        "e9e65689ecff7dd714e7e1f349c442c02f4ad4992c77532d58c45a7bd2f2aaa1",
    )?;
    natural_settlement_mutant()?;
    for mode in ["transient", "active", "unknown", "expired", "late"] {
        let mut host = Natural {
            clock: 0,
            mode,
            events: Vec::new(),
            queries: 0,
        };
        let result = browser::observe(&mut host, Duration::from_secs(15));
        if mode == "transient" {
            let result = result?;
            assert!(result.elapsed_ns >= 250_000_000);
            assert_eq!(result.bytes, 1234);
            assert!(
                host.events.iter().position(|v| *v == "peak")
                    < host.events.iter().position(|v| *v == "cleanup")
            );
        } else {
            assert!(result.is_err());
            assert!(!host.events.contains(&"peak"));
            if matches!(mode, "active" | "unknown") {
                assert!((15_000_000_000..15_050_000_000).contains(&host.clock));
            } else {
                assert_eq!(host.queries, usize::from(mode == "late"));
            }
        }
        assert_eq!(host.events.iter().filter(|v| **v == "cleanup").count(), 1);
        assert_eq!(
            &host.events[host.events.len() - 2..],
            ["process-close", "job-close"]
        );
    }
    Ok(())
}

fn natural_settlement_mutant() -> TestResult {
    // Mutate the actual shipped Rust function to accept a sample without
    // observing whether the retained descendant tree settled naturally.
    let current = fs::read_to_string(root().join("tools/xtask/src/ci/browser_benchmark.rs"))?;
    let start = current
        .find("pub fn observe(")
        .ok_or("Observer function missing")?;
    let end = current[start..]
        .find("\npub fn positive_samples(")
        .ok_or("Observer function boundary missing")?
        + start;
    let current = &current[start..end];
    let query = "let empty = host.natural(deadline)?;";
    assert_eq!(current.matches(query).count(), 1);
    let mutant = current.replacen(query, "let empty = true;", 1);
    let source = format!(
        r#"
use numera_xtask::ci::{{Result,fail,browser_benchmark::{{Observer,Sample}}}};
use serde_json::{{Value,json}};
use std::time::Duration;
{mutant}
#[derive(Default)] struct Host {{ clock:u64, queries:usize, peak:bool, cleanup:usize, reaped:bool, closed:bool }}
impl Observer for Host {{
 fn register(&mut self)->Result<()>{{Ok(())}}
 fn ticks(&mut self)->u64{{self.clock}}
 fn spawn(&mut self)->Result<()>{{Ok(())}}
 fn communicate(&mut self,_:u64)->Result<Value>{{Ok(json!({{"complete":true}}))}}
 fn natural(&mut self,_:u64)->Result<bool>{{self.queries+=1;Ok(false)}}
 fn pause(&mut self)->Result<()>{{self.clock+=10_000_000;Ok(())}}
 fn peak(&mut self)->Result<(String,u64)>{{self.peak=true;Ok(("peak-job-committed-bytes".into(),1234))}}
 fn cleanup(&mut self)->Result<()>{{self.cleanup+=1;Ok(())}}
 fn reap(&mut self)->Result<()>{{self.reaped=true;Ok(())}}
 fn close(&mut self)->Result<()>{{self.closed=true;Ok(())}}
}}
fn main() {{
 let mut broken=Host::default();
 let sample=observe(&mut broken,Duration::from_secs(15)).expect("mutant accepts live descendants");
 assert_eq!(sample.elapsed_ns,0);assert_eq!(broken.queries,0);assert!(broken.peak);
 assert_eq!(broken.cleanup,1);assert!(broken.reaped&&broken.closed);
 let mut fixed=Host::default();
 assert!(numera_xtask::ci::browser_benchmark::observe(&mut fixed,Duration::from_secs(15)).is_err());
 assert!(fixed.queries>0);assert!(!fixed.peak);assert_eq!(fixed.cleanup,1);
 assert!(fixed.reaped&&fixed.closed);
}}
"#
    );
    let temp = Temp::new("Rust natural-settlement mutant")?;
    let binary = compile(&temp, &source)?;
    let result = ci::capture(
        &mut Command::new(binary),
        Duration::from_secs(5),
        Duration::from_secs(5),
        65536,
    )?;
    assert_eq!(
        result.exit,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}
