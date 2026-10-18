// SPDX-License-Identifier: Apache-2.0 OR MIT
//! HID record and replay: the conformance harness for plugins.
//!
//! [`RecordingHid`] wraps any [`HidBackend`] and logs every operation a
//! session performs, with what the device answered, into a [`HidTrace`]
//! (JSON, bytes in hex). [`ReplayHid`] plays a trace back without hardware:
//! it enumerates the recorded collections and answers reads and GET_REPORTs
//! with the recorded responses, in order, per collection (and report id),
//! while logging what the plugin sends. [`compare`] then lines up what the
//! plugin sent during the recording and during the replay.
//!
//! A trace replays exactly when the plugin's output depends only on the
//! device's answers and on the canvas: record with a solid colour
//! (`huedeck light`), not with a time-based pattern.

use crate::error::{HalError, HalResult};
use crate::hid::{HidBackend, HidEndpointInfo, HidHandle};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

/// Trace format version.
pub const TRACE_VERSION: u32 = 1;

/// Bytes as a lowercase hex string in JSON.
mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            out.push_str(&format!("{b:02x}"));
        }
        s.serialize_str(&out)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        if text.len() % 2 != 0 {
            return Err(serde::de::Error::custom("odd number of hex digits"));
        }
        (0..text.len())
            .step_by(2)
            .map(|i| {
                text.get(i..i + 2)
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                    .ok_or_else(|| serde::de::Error::custom("invalid hex"))
            })
            .collect()
    }
}

/// What the device answered: the bytes, or the error text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// Bytes returned (report id first).
    Data(#[serde(with = "hex_bytes")] Vec<u8>),
    /// The operation failed.
    Error(String),
}

/// One HID operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TraceOp {
    /// The collection was opened.
    Open,
    /// Opening the collection failed.
    OpenFailed {
        /// Why.
        error: String,
    },
    /// Output report sent.
    Write {
        /// Report, id first.
        #[serde(with = "hex_bytes")]
        data: Vec<u8>,
        /// The error, when the write failed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Feature report sent.
    SendFeature {
        /// Report, id first.
        #[serde(with = "hex_bytes")]
        data: Vec<u8>,
        /// The error, when the send failed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// GET_REPORT(Feature) of `len` bytes.
    GetFeature {
        /// Report id asked for.
        report_id: u8,
        /// Buffer length.
        len: usize,
        /// The answer.
        answer: Answer,
    },
    /// GET_REPORT(Input) of `len` bytes.
    GetInput {
        /// Report id asked for.
        report_id: u8,
        /// Buffer length.
        len: usize,
        /// The answer.
        answer: Answer,
    },
    /// Interrupt IN read (`Data([])` = timed out).
    Read {
        /// Buffer length.
        len: usize,
        /// Timeout in milliseconds.
        timeout_ms: i32,
        /// The answer.
        answer: Answer,
    },
    /// Input queue flushed.
    ClearReadBuffer,
}

/// Which kind of report a packet was sent as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SentKind {
    /// Output report (`write`).
    Output,
    /// Feature report (`send_feature_report`).
    Feature,
}

/// A packet sent to a device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sent {
    /// Report kind.
    pub kind: SentKind,
    /// Report, id first.
    #[serde(with = "hex_bytes")]
    pub bytes: Vec<u8>,
}

impl TraceOp {
    /// The report this operation sent to the device, if it sent one.
    pub fn sent(&self) -> Option<(SentKind, &[u8])> {
        match self {
            TraceOp::Write { data, .. } => Some((SentKind::Output, data)),
            TraceOp::SendFeature { data, .. } => Some((SentKind::Feature, data)),
            _ => None,
        }
    }
}

/// One recorded operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceEvent {
    /// Microseconds since the recording started.
    pub t_us: u64,
    /// Collection path.
    pub path: String,
    /// What happened.
    #[serde(flatten)]
    pub op: TraceOp,
}

/// A device a recording drove: its key and the paths of its collections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TracedDevice {
    /// `vid:pid:serial`.
    pub key: String,
    /// USB vendor id.
    pub vid: u16,
    /// USB product id.
    pub pid: u16,
    /// Its collections, in enumeration order.
    pub paths: Vec<String>,
    /// `Render()` calls of its session; a replay renders as many.
    #[serde(default)]
    pub frames: u64,
    /// Components assigned per channel (each a JSON array of component instances).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub channels: BTreeMap<String, serde_json::Value>,
    /// Sessions the recording ran for the device; more than one (a hotplug
    /// restart) cannot be replayed in lockstep.
    #[serde(default)]
    pub sessions: u32,
}

/// Which feature reports the recording's sessions read before `Initialize`
/// and after `Shutdown` (those reads are in the trace too).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceRestoreCheck {
    /// None.
    #[default]
    Off,
    /// Every feature report the report descriptor declares.
    Declared,
    /// These `(report id, length)` pairs.
    Reports(Vec<(u8, usize)>),
}

/// Session options of the recording that change what the plugins send.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceOptions {
    /// Global brightness `0..=100`.
    #[serde(default = "full_brightness")]
    pub brightness: u8,
    /// `Shutdown(false)` was skipped.
    #[serde(default)]
    pub skip_shutdown: bool,
    /// Restore-check reads.
    #[serde(default)]
    pub restore_check: TraceRestoreCheck,
}

fn full_brightness() -> u8 {
    100
}

impl Default for TraceOptions {
    fn default() -> Self {
        Self {
            brightness: 100,
            skip_shutdown: false,
            restore_check: TraceRestoreCheck::Off,
        }
    }
}

/// A recording: the collections and every operation on them, in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HidTrace {
    /// [`TRACE_VERSION`].
    pub version: u32,
    /// Free-form description (how it was recorded: command, colour, ...).
    #[serde(default)]
    pub note: String,
    /// Solid canvas colour the recording rendered (`#rrggbb`); a trace of a
    /// moving pattern has none and cannot be replayed exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<String>,
    /// Plugin parameter overrides of the recording.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub parameters: serde_json::Map<String, serde_json::Value>,
    /// What `@SignalRGB/systeminfo` reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_info: Option<crate::smbios::SystemInfo>,
    /// Session options that change what the plugins send.
    #[serde(default)]
    pub options: TraceOptions,
    /// The devices driven.
    #[serde(default)]
    pub devices: Vec<TracedDevice>,
    /// The collections enumerated, as the backend described them.
    pub endpoints: Vec<HidEndpointInfo>,
    /// Operations in order.
    pub events: Vec<TraceEvent>,
}

impl Default for HidTrace {
    fn default() -> Self {
        Self {
            version: TRACE_VERSION,
            note: String::new(),
            canvas: None,
            parameters: serde_json::Map::new(),
            system_info: None,
            options: TraceOptions::default(),
            devices: Vec::new(),
            endpoints: Vec::new(),
            events: Vec::new(),
        }
    }
}

impl HidTrace {
    /// Parse a trace, checking the version.
    pub fn from_json(text: &str) -> HalResult<Self> {
        let t: Self =
            serde_json::from_str(text).map_err(|e| HalError::InvalidArgument(format!("trace: {e}")))?;
        if t.version != TRACE_VERSION {
            return Err(HalError::InvalidArgument(format!(
                "trace version {} (this build reads {TRACE_VERSION})",
                t.version
            )));
        }
        Ok(t)
    }

    /// Reports sent to the collection at `path`, in order.
    pub fn sent_to(&self, path: &str) -> Vec<(SentKind, &[u8])> {
        self.events
            .iter()
            .filter(|e| e.path == path)
            .filter_map(|e| e.op.sent())
            .collect()
    }
}

#[derive(Debug)]
struct Log {
    started: Instant,
    trace: Mutex<HidTrace>,
}

impl Log {
    fn new(note: String) -> Arc<Self> {
        Arc::new(Self {
            started: Instant::now(),
            trace: Mutex::new(HidTrace {
                note,
                ..HidTrace::default()
            }),
        })
    }

    fn trace(&self) -> MutexGuard<'_, HidTrace> {
        self.trace.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn push(&self, path: &str, op: TraceOp) {
        let t_us = self.started.elapsed().as_micros() as u64;
        self.trace().events.push(TraceEvent {
            t_us,
            path: path.to_string(),
            op,
        });
    }

    fn note_endpoints(&self, infos: &[HidEndpointInfo]) {
        let mut t = self.trace();
        for i in infos {
            if !t.endpoints.iter().any(|e| e.path == i.path) {
                t.endpoints.push(i.clone());
            }
        }
    }
}

fn answer(r: &HalResult<usize>, buf: &[u8]) -> Answer {
    match r {
        Ok(n) => Answer::Data(buf[..(*n).min(buf.len())].to_vec()),
        Err(e) => Answer::Error(e.to_string()),
    }
}

/// Records every operation on an inner backend (see the module docs).
pub struct RecordingHid {
    inner: Arc<dyn HidBackend>,
    log: Arc<Log>,
}

impl std::fmt::Debug for RecordingHid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordingHid").finish_non_exhaustive()
    }
}

impl RecordingHid {
    /// Record what goes through `inner`; `note` describes the recording.
    pub fn new(inner: Arc<dyn HidBackend>, note: impl Into<String>) -> Self {
        Self {
            inner,
            log: Log::new(note.into()),
        }
    }

    /// Add collections the caller enumerated directly on the inner backend
    /// (the opened ones are added automatically).
    pub fn note_endpoints(&self, infos: &[HidEndpointInfo]) {
        self.log.note_endpoints(infos);
    }

    /// Record that the session `key` drives the collections `endpoints`.
    pub fn note_device(&self, key: &str, vid: u16, pid: u16, endpoints: &[HidEndpointInfo]) {
        self.log.note_endpoints(endpoints);
        let mut t = self.log.trace();
        if !t.devices.iter().any(|d| d.key == key) {
            t.devices.push(TracedDevice {
                key: key.to_string(),
                vid,
                pid,
                paths: endpoints.iter().map(|e| e.path.clone()).collect(),
                frames: 0,
                channels: BTreeMap::new(),
                sessions: 0,
            });
        }
    }

    /// The components on the channels of `key` (channel name -> JSON array of instances).
    pub fn set_channels(&self, key: &str, channels: BTreeMap<String, serde_json::Value>) {
        if let Some(d) = self.log.trace().devices.iter_mut().find(|d| d.key == key) {
            d.channels = channels;
        }
    }

    /// What `@SignalRGB/systeminfo` reports during the recording.
    pub fn set_system_info(&self, info: Option<crate::smbios::SystemInfo>) {
        self.log.trace().system_info = info;
    }

    /// A session of `key` ended after `frames` `Render()` calls.
    pub fn add_session(&self, key: &str, frames: u64) {
        if let Some(d) = self.log.trace().devices.iter_mut().find(|d| d.key == key) {
            d.frames = frames;
            d.sessions += 1;
        }
    }

    /// Session options that change what the plugins send.
    pub fn set_options(&self, options: TraceOptions) {
        self.log.trace().options = options;
    }

    /// What the canvas showed and which parameters were overridden, for the replay.
    pub fn set_conditions(
        &self,
        canvas: Option<String>,
        parameters: serde_json::Map<String, serde_json::Value>,
    ) {
        let mut t = self.log.trace();
        t.canvas = canvas;
        t.parameters = parameters;
    }

    /// The recording so far.
    pub fn trace(&self) -> HidTrace {
        self.log.trace().clone()
    }
}

impl HidBackend for RecordingHid {
    fn enumerate(&self) -> HalResult<Vec<HidEndpointInfo>> {
        let infos = self.inner.enumerate()?;
        self.log.note_endpoints(&infos);
        Ok(infos)
    }

    fn open(&self, path: &str) -> HalResult<Box<dyn HidHandle>> {
        let inner = match self.inner.open(path) {
            Ok(h) => h,
            Err(e) => {
                self.log.push(path, TraceOp::OpenFailed { error: e.to_string() });
                return Err(e);
            }
        };
        self.log.note_endpoints(std::slice::from_ref(inner.info()));
        self.log.push(path, TraceOp::Open);
        Ok(Box::new(RecordingHandle {
            inner,
            log: Arc::clone(&self.log),
        }))
    }
}

struct RecordingHandle {
    inner: Box<dyn HidHandle>,
    log: Arc<Log>,
}

impl RecordingHandle {
    fn path(&self) -> String {
        self.inner.info().path.clone()
    }
}

impl HidHandle for RecordingHandle {
    fn info(&self) -> &HidEndpointInfo {
        self.inner.info()
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        let r = self.inner.write(data);
        let error = r.as_ref().err().map(|e| e.to_string());
        self.log.push(
            &self.path(),
            TraceOp::Write {
                data: data.to_vec(),
                error,
            },
        );
        r
    }

    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> HalResult<usize> {
        let r = self.inner.read_timeout(buf, timeout_ms);
        self.log.push(
            &self.path(),
            TraceOp::Read {
                len: buf.len(),
                timeout_ms,
                answer: answer(&r, buf),
            },
        );
        r
    }

    fn send_feature_report(&mut self, data: &[u8]) -> HalResult<usize> {
        let r = self.inner.send_feature_report(data);
        let error = r.as_ref().err().map(|e| e.to_string());
        self.log.push(
            &self.path(),
            TraceOp::SendFeature {
                data: data.to_vec(),
                error,
            },
        );
        r
    }

    fn get_feature_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        let report_id = buf.first().copied().unwrap_or(0);
        let r = self.inner.get_feature_report(buf);
        self.log.push(
            &self.path(),
            TraceOp::GetFeature {
                report_id,
                len: buf.len(),
                answer: answer(&r, buf),
            },
        );
        r
    }

    fn get_input_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        let report_id = buf.first().copied().unwrap_or(0);
        let r = self.inner.get_input_report(buf);
        self.log.push(
            &self.path(),
            TraceOp::GetInput {
                report_id,
                len: buf.len(),
                answer: answer(&r, buf),
            },
        );
        r
    }

    fn clear_read_buffer(&mut self) -> HalResult<()> {
        let r = self.inner.clear_read_buffer();
        self.log.push(&self.path(), TraceOp::ClearReadBuffer);
        r
    }
}

/// Recorded answers of one collection, consumed in order.
#[derive(Debug, Default)]
struct Answers {
    features: BTreeMap<u8, VecDeque<Answer>>,
    inputs: BTreeMap<u8, VecDeque<Answer>>,
    reads: VecDeque<Answer>,
    /// Outcomes of the opens (`Some(error)` = failed).
    opens: VecDeque<Option<String>>,
    /// Outcomes of the writes and feature sends, in order.
    sends: VecDeque<Option<String>>,
}

/// Plays a [`HidTrace`] back (see the module docs).
pub struct ReplayHid {
    endpoints: Vec<HidEndpointInfo>,
    answers: Arc<Mutex<BTreeMap<String, Answers>>>,
    log: Arc<Log>,
}

impl std::fmt::Debug for ReplayHid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayHid")
            .field("endpoints", &self.endpoints.len())
            .finish_non_exhaustive()
    }
}

impl ReplayHid {
    /// A backend answering from `trace`.
    pub fn new(trace: &HidTrace) -> Self {
        let mut answers: BTreeMap<String, Answers> = BTreeMap::new();
        for e in &trace.events {
            let a = answers.entry(e.path.clone()).or_default();
            match &e.op {
                TraceOp::GetFeature {
                    report_id, answer, ..
                } => {
                    a.features
                        .entry(*report_id)
                        .or_default()
                        .push_back(answer.clone());
                }
                TraceOp::GetInput {
                    report_id, answer, ..
                } => {
                    a.inputs.entry(*report_id).or_default().push_back(answer.clone());
                }
                TraceOp::Read { answer, .. } => a.reads.push_back(answer.clone()),
                TraceOp::Open => a.opens.push_back(None),
                TraceOp::OpenFailed { error } => a.opens.push_back(Some(error.clone())),
                TraceOp::Write { error, .. } | TraceOp::SendFeature { error, .. } => {
                    a.sends.push_back(error.clone());
                }
                TraceOp::ClearReadBuffer => {}
            }
        }
        Self {
            endpoints: trace.endpoints.clone(),
            answers: Arc::new(Mutex::new(answers)),
            log: Log::new(format!("replay of: {}", trace.note)),
        }
    }

    /// What the plugin did during the replay.
    pub fn trace(&self) -> HidTrace {
        let mut t = self.log.trace().clone();
        t.endpoints = self.endpoints.clone();
        t
    }
}

impl HidBackend for ReplayHid {
    fn enumerate(&self) -> HalResult<Vec<HidEndpointInfo>> {
        Ok(self.endpoints.clone())
    }

    fn open(&self, path: &str) -> HalResult<Box<dyn HidHandle>> {
        let info = self
            .endpoints
            .iter()
            .find(|e| e.path == path)
            .cloned()
            .ok_or_else(|| HalError::NotFound(path.to_string()))?;
        let outcome = {
            let mut all = self.answers.lock().unwrap_or_else(|e| e.into_inner());
            all.get_mut(path).and_then(|a| a.opens.pop_front())
        };
        let error = match outcome {
            Some(None) => None,
            Some(Some(e)) => Some(e),
            None => Some("replay: the recording did not open this collection here".to_string()),
        };
        if let Some(error) = error {
            self.log.push(path, TraceOp::OpenFailed { error: error.clone() });
            return Err(HalError::Backend(error));
        }
        self.log.push(path, TraceOp::Open);
        Ok(Box::new(ReplayHandle {
            info,
            answers: Arc::clone(&self.answers),
            log: Arc::clone(&self.log),
        }))
    }
}

struct ReplayHandle {
    info: HidEndpointInfo,
    answers: Arc<Mutex<BTreeMap<String, Answers>>>,
    log: Arc<Log>,
}

impl ReplayHandle {
    /// The recorded outcome of the next write or feature send.
    fn next_send(&self) -> Option<String> {
        let mut all = self.answers.lock().unwrap_or_else(|e| e.into_inner());
        all.get_mut(&self.info.path)
            .and_then(|a| a.sends.pop_front())
            .flatten()
    }

    /// The next recorded answer picked by `pick`, copied into `buf`.
    fn next(
        &self,
        buf: &mut [u8],
        pick: impl FnOnce(&mut Answers) -> Option<Answer>,
    ) -> (HalResult<usize>, Answer) {
        let next = {
            let mut all = self.answers.lock().unwrap_or_else(|e| e.into_inner());
            all.get_mut(&self.info.path).and_then(pick)
        };
        match next {
            Some(Answer::Data(bytes)) => {
                let n = bytes.len().min(buf.len());
                buf[..n].copy_from_slice(&bytes[..n]);
                (Ok(n), Answer::Data(bytes[..n].to_vec()))
            }
            Some(Answer::Error(e)) => (Err(HalError::Backend(e.clone())), Answer::Error(e)),
            None => {
                let e = "replay: the trace has no more answers here".to_string();
                (Err(HalError::Backend(e.clone())), Answer::Error(e))
            }
        }
    }
}

impl HidHandle for ReplayHandle {
    fn info(&self) -> &HidEndpointInfo {
        &self.info
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        let error = self.next_send();
        self.log.push(
            &self.info.path,
            TraceOp::Write {
                data: data.to_vec(),
                error: error.clone(),
            },
        );
        match error {
            Some(e) => Err(HalError::Backend(e)),
            None => Ok(data.len()),
        }
    }

    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> HalResult<usize> {
        // An exhausted trace reads as a timeout, as a quiet device would.
        let (r, answer) = match self.next(buf, |a| a.reads.pop_front()) {
            (Err(_), Answer::Error(e)) if e.starts_with("replay:") => (Ok(0), Answer::Data(Vec::new())),
            other => other,
        };
        self.log.push(
            &self.info.path,
            TraceOp::Read {
                len: buf.len(),
                timeout_ms,
                answer,
            },
        );
        r
    }

    fn send_feature_report(&mut self, data: &[u8]) -> HalResult<usize> {
        let error = self.next_send();
        self.log.push(
            &self.info.path,
            TraceOp::SendFeature {
                data: data.to_vec(),
                error: error.clone(),
            },
        );
        match error {
            Some(e) => Err(HalError::Backend(e)),
            None => Ok(data.len()),
        }
    }

    fn get_feature_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        let report_id = buf.first().copied().unwrap_or(0);
        let len = buf.len();
        let (r, answer) = self.next(buf, |a| {
            a.features.get_mut(&report_id).and_then(VecDeque::pop_front)
        });
        self.log.push(
            &self.info.path,
            TraceOp::GetFeature {
                report_id,
                len,
                answer,
            },
        );
        r
    }

    fn get_input_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        let report_id = buf.first().copied().unwrap_or(0);
        let len = buf.len();
        let (r, answer) = self.next(buf, |a| {
            a.inputs.get_mut(&report_id).and_then(VecDeque::pop_front)
        });
        self.log.push(
            &self.info.path,
            TraceOp::GetInput {
                report_id,
                len,
                answer,
            },
        );
        r
    }

    fn clear_read_buffer(&mut self) -> HalResult<()> {
        self.log.push(&self.info.path, TraceOp::ClearReadBuffer);
        Ok(())
    }
}

/// Where a replay first sent something else than the recording.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Divergence {
    /// Collection path.
    pub path: String,
    /// Index among the packets sent to that collection.
    pub index: usize,
    /// What the recording sent (`None`: the recording sent fewer packets).
    pub recorded: Option<Sent>,
    /// What the replay sent (`None`: the replay sent fewer packets).
    pub replayed: Option<Sent>,
}

/// The first difference between the packets each collection received in the
/// recording and in the replay (a replay renders as many frames as the
/// recording, so the sequences should be equal).
pub fn compare(recorded: &HidTrace, replayed: &HidTrace) -> Vec<Divergence> {
    let mut paths: Vec<&str> = recorded
        .events
        .iter()
        .chain(&replayed.events)
        .map(|e| e.path.as_str())
        .collect();
    paths.sort_unstable();
    paths.dedup();
    let mut out = Vec::new();
    for path in paths {
        let (a, b) = (recorded.sent_to(path), replayed.sent_to(path));
        let first = (0..a.len().max(b.len())).find(|&i| a.get(i) != b.get(i));
        if let Some(index) = first {
            let packet = |p: &(SentKind, &[u8])| Sent {
                kind: p.0,
                bytes: p.1.to_vec(),
            };
            out.push(Divergence {
                path: path.to_string(),
                index,
                recorded: a.get(index).map(packet),
                replayed: b.get(index).map(packet),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::ReportLengths;
    use crate::mock::MockHid;

    fn device() -> (Arc<MockHid>, HidEndpointInfo) {
        let info = MockHid::endpoint_info(1, 2, "T", 0, 0xFF00, 1, 0, ReportLengths::default());
        let mock = MockHid::new().add_endpoint(info.clone());
        mock.set_feature_report(&info.path, vec![0x52, 7, 8]).unwrap();
        mock.script_read(&info.path, vec![0x01, 0xAA]).unwrap();
        (Arc::new(mock), info)
    }

    /// What a plugin might do: read config, answer with a packet built from it, poll once.
    fn exercise(backend: &dyn HidBackend, path: &str, colour: u8) -> Vec<u8> {
        let mut h = backend.open(path).unwrap();
        let mut feature = [0x52u8, 0, 0];
        let n = h.get_feature_report(&mut feature).unwrap();
        h.send_feature_report(&[0x52, feature[1], colour]).unwrap();
        let mut input = [0u8; 4];
        let r = h.read_timeout(&mut input, 10).unwrap();
        h.write(&[0x00, input[1], colour]).unwrap();
        vec![n as u8, r as u8]
    }

    #[test]
    fn a_recording_replays_identically_and_differences_are_found() {
        let (mock, info) = device();
        let rec = RecordingHid::new(mock, "unit test, colour 9");
        assert_eq!(exercise(&rec, &info.path, 9), vec![3, 2]);
        let trace = rec.trace();
        assert_eq!(trace.endpoints.len(), 1);
        // Round trip through JSON (bytes as hex).
        let json = serde_json::to_string(&trace).unwrap();
        assert!(
            json.contains("\"op\":\"get_feature\"") && json.contains("520708"),
            "{json}"
        );
        let trace = HidTrace::from_json(&json).unwrap();

        let replay = ReplayHid::new(&trace);
        assert_eq!(replay.enumerate().unwrap(), trace.endpoints);
        assert_eq!(
            exercise(&replay, &info.path, 9),
            vec![3, 2],
            "answers come from the trace"
        );
        assert!(compare(&trace, &replay.trace()).is_empty());

        // A plugin that now sends another colour diverges at the first packet.
        let replay = ReplayHid::new(&trace);
        exercise(&replay, &info.path, 10);
        let d = compare(&trace, &replay.trace());
        assert_eq!(d.len(), 1);
        let feature = |bytes: Vec<u8>| Sent {
            kind: SentKind::Feature,
            bytes,
        };
        assert_eq!(
            (d[0].index, d[0].recorded.clone(), d[0].replayed.clone()),
            (
                0,
                Some(feature(vec![0x52, 7, 9])),
                Some(feature(vec![0x52, 7, 10]))
            )
        );

        // Answers run out: GET_REPORT fails, reads time out.
        let replay = ReplayHid::new(&trace);
        let mut h = replay.open(&info.path).unwrap();
        let mut buf = [0x52u8, 0, 0];
        h.get_feature_report(&mut buf).unwrap();
        assert!(h.get_feature_report(&mut buf).is_err());
        let mut input = [0u8; 4];
        h.read_timeout(&mut input, 0).unwrap();
        assert_eq!(h.read_timeout(&mut input, 0).unwrap(), 0);
    }

    #[test]
    fn failures_and_report_kinds_are_replayed() {
        let (mock, info) = device();
        let rec = RecordingHid::new(mock.clone(), "failures");
        assert!(rec.open("mock://nowhere").is_err());
        let mut h = rec.open(&info.path).unwrap();
        mock.endpoint(&info.path).unwrap().lock().unwrap().fail_writes = true;
        assert!(h.write(&[0, 1]).is_err());
        let trace = rec.trace();
        assert!(matches!(trace.events[0].op, TraceOp::OpenFailed { .. }));

        let replay = ReplayHid::new(&trace);
        let mut r = replay.open(&info.path).unwrap();
        assert!(r.write(&[0, 1]).is_err(), "the recorded failure is replayed");
        assert!(replay.open(&info.path).is_err(), "only one open was recorded");
        assert!(compare(&trace, &replay.trace()).is_empty());

        // The same bytes sent as a feature report are a different packet.
        let replay = ReplayHid::new(&trace);
        let mut r = replay.open(&info.path).unwrap();
        let _ = r.send_feature_report(&[0, 1]);
        let d = compare(&trace, &replay.trace());
        assert_eq!(d[0].recorded.as_ref().map(|s| s.kind), Some(SentKind::Output));
        assert_eq!(d[0].replayed.as_ref().map(|s| s.kind), Some(SentKind::Feature));
    }

    #[test]
    fn foreign_versions_are_refused() {
        let t = HidTrace {
            version: 99,
            ..HidTrace::default()
        };
        assert!(HidTrace::from_json(&serde_json::to_string(&t).unwrap()).is_err());
        assert!(HidTrace::from_json("{").is_err());
    }
}
