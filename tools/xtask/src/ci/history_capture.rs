//! Exact caller-only bridge for the reviewed pre-observer capture snapshot.
//! The original direct capture test and selected defect bytes stay unchanged.
use super::{
    Result, fail,
    history::{Case, Source},
    history_compat::{Replacement, caller_replacement, checked_caller},
};
use std::{collections::BTreeMap, path::Path};

const CAPTURE: &str = "engine-rust/src/ownership/capture.rs";
const UNITS: &str = "engine-rust/src/ownership/capture_units.rs";
const ADAPTER: &str = "engine-rust/src/browser/adapter.rs";
const PRIMARY: &str = "aa93ca78468acf0874bf58cf6e1fbd3ca70c7990146ce36e92ff4c3285c51e54";

const CURRENT_RUN: &str = r#"    /// Settle reported SDK trees as soon as the producer tree ends, before waiting
    /// for EOF from writers those SDKs may have inherited. A live producer is
    /// ended first so it cannot add another ownership transfer after proof.
    pub fn run(
        &mut self,
        command: &mut Command,
        limits: super::capture::Limits,
        cancelled: impl FnMut() -> bool,
    ) -> Result<super::capture::Output, super::capture::Failure> {
        self.run_observed(command, limits, cancelled, |_, _| {})
    }
    pub fn run_observed(
        &mut self,
        command: &mut Command,
        limits: super::capture::Limits,
        cancelled: impl FnMut() -> bool,
        observe: impl FnMut(super::capture::Observation, &super::tree::OwnedTree),
    ) -> Result<super::capture::Output, super::capture::Failure> {
        self.run_observed_streamed(command, limits, cancelled, observe, |_, _| {})
    }
    pub fn run_observed_streamed(
        &mut self,
        command: &mut Command,
        limits: super::capture::Limits,
        mut cancelled: impl FnMut() -> bool,
        observe: impl FnMut(super::capture::Observation, &super::tree::OwnedTree),
        on_chunk: impl FnMut(usize, &[u8]),
    ) -> Result<super::capture::Output, super::capture::Failure> {
        use std::cell::Cell;
        if self.path.is_none() {
            return Err(io::Error::other("Capture receipt was already settled").into());
        }
        if let Some(producer) = &self.producer
            && !super::recorded_tree::empty(producer)?
        {
            return Err(io::Error::other("Capture still owns an unsettled producer").into());
        }
        let failed = Cell::new(false);
        let mut receipt_error = None;
        let mut attempted = false;
        let cleanup = limits.cleanup;
        let captured = super::capture::run_observed_then(
            command,
            limits,
            || cancelled() || failed.get(),
            observe,
            |settled, tree, remaining| {
                let mut record = tree.record();
                record["pidToken"] = record["token"].take();
                self.producer = Some(record);
                attempted = true;
                let settlement = if settled {
                    self.settle(remaining)
                } else {
                    Err(io::Error::other(
                        "Captured producer cleanup remains unverified; its receipt is retained",
                    ))
                };
                if let Err(error) = settlement {
                    receipt_error = Some(error);
                    failed.set(true);
                }
            },
            on_chunk,
        );
        if !attempted && let Err(error) = self.settle(cleanup) {
            receipt_error = Some(error);
        }
        if let Some(error) = receipt_error {
            return Err(match captured {
                Err(failure) => failure.context(error),
                Ok(_) => error.into(),
            });
        }
        captured
    }"#;

const LEGACY_RUN: &str = r#"    /// Historical caller bridge for the direct-capture proof. The selected
    /// capture implementation owns its real limits and cancellation; nested
    /// receipts remain mandatory afterward. New observer APIs are unavailable.
    pub fn run(
        &mut self,
        command: &mut Command,
        limits: super::capture::Limits,
        cancelled: impl FnMut() -> bool,
    ) -> Result<super::capture::Output, super::capture::Failure> {
        if self.path.is_none() {
            return Err(io::Error::other("Capture receipt was already settled").into());
        }
        if let Some(producer) = &self.producer
            && !super::recorded_tree::empty(producer)?
        {
            return Err(io::Error::other("Capture still owns an unsettled producer").into());
        }
        let cleanup = limits.cleanup;
        let captured = super::capture::run_controlled(command, limits, cancelled);
        if let Err(failure) = &captured
            && let Some(mut record) = failure.retained_record()
        {
            record["pidToken"] = record["token"].take();
            self.producer = Some(record);
        }
        let settlement = self.settle(cleanup);
        match (captured, settlement) {
            (Err(failure), _) => Err(failure),
            (Ok(_), Err(error)) => Err(error.into()),
            (Ok(output), Ok(())) => Ok(output),
        }
    }"#;

pub(super) fn additional(root: &Path, case: &Case) -> Result<Vec<(String, Replacement)>> {
    if case.target != "command_capture_lifecycle"
        || case.test != "bounded_capture_drains_both_pipes_and_settles_descendants_on_every_exit"
    {
        return Ok(Vec::new());
    }
    if case.commit != "b9d7399732c140d6141f37633486fffd90a2e30a"
        || case.files != BTreeMap::from([(CAPTURE.into(), PRIMARY.into())])
        || !case.companions.is_empty()
        || case.platform.is_some()
    {
        return fail("Unreviewed historical capture caller selector");
    }
    let source = Source {
        commit: case.commit.clone(),
        sha256: PRIMARY.into(),
    };
    let bytes = checked_caller(
        root,
        case,
        UNITS,
        "fe8dd952159e82a46647a02f3a06742ff7e07de08db6817d5a10880242a1a375",
    )?;
    let mut units = caller_replacement(
        case,
        UNITS,
        &bytes,
        &source,
        &[
            (
                "use super::unit_report;",
                "pub(crate) use super::capture_pipe::Pipe as HistoricalCapturePipe;\nuse super::unit_report;",
            ),
            (CURRENT_RUN, LEGACY_RUN),
        ],
    )?;
    if super::sha(&units.bytes)
        != "c0ce84fc39ef1af75e56033fe485b233743a5b29d07a5a2c88d2b7289189808f"
    {
        return fail("Historical capture Units bridge differs from its actual proof");
    }
    let bytes = checked_caller(
        root,
        case,
        ADAPTER,
        "3c16bc7c64362cd8a97443d143175d7fa8afa31fdd77395f847af4d004cb5c6f",
    )?;
    let mut adapter = caller_replacement(
        case,
        ADAPTER,
        &bytes,
        &source,
        &[
            (
                "    pipe: crate::ownership::capture::Pipe,",
                "    pipe: crate::ownership::capture_units::HistoricalCapturePipe,",
            ),
            (
                "        let pipe = crate::ownership::capture::Pipe::Out(stdout);",
                "        let pipe = crate::ownership::capture_units::HistoricalCapturePipe::Out(stdout);",
            ),
        ],
    )?;
    if super::sha(&adapter.bytes)
        != "029549a0e5badba41fd2f1b48ce836c18aa09b9417f49526f15b91934a5c5699"
    {
        return fail("Historical capture pipe alias differs from its actual proof");
    }
    for replacement in [&mut units, &mut adapter] {
        replacement.receipt["historical_source_path"] = CAPTURE.into();
        replacement.receipt["selected_primary_bytes_unchanged"] = true.into();
        replacement.receipt["caller_only_current_green_probe_sha256"] =
            "a7b104c8ab212a471a8fe64be3712e3f4507e724f97d458ecf64d97fc59b1924".into();
        replacement.receipt["reason"] = "The exact direct-capture target and fixture compiler call original run/run_controlled. Only later unselected observer adapters are unavailable; the pipe alias is the same capture_pipe::Pipe enum. Preserve limits, cancellation, mandatory receipt settlement and actual retained producer ownership; synthesize no observations.".into();
        replacement.receipt["secondary_diagnostic_limitation"] = "Old Failure has no context API. If capture and receipt settlement both fail, the original capture error remains fatal and Units Drop retains the unsettled receipt; secondary error attachment and modern before-EOF settlement are not claimed for this exact historical graph.".into();
    }
    Ok(vec![(UNITS.into(), units), (ADAPTER.into(), adapter)])
}
