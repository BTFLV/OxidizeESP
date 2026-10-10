//! Serial monitor.
//!
//! espflash's monitor is part of its command-line interface and is bound to a
//! terminal, so the monitor is implemented here on top of the serial port
//! abstraction. Address resolution follows the approach used by espflash:
//! hexadecimal code addresses in the output are looked up in the firmware ELF
//! using DWARF debug information.

use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
    sync::{Arc, LazyLock, mpsc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use addr2line::{
    Context,
    gimli::{self, EndianArcSlice, RunTimeEndian},
};
use object::{Object, ObjectSection, ObjectSegment, ObjectSymbol};
use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Error, ErrorKind, Result, error::display_name, fsutil, hardware::SerialLink};

/// Reassembles UTF-8 text from arbitrary byte chunks. Incomplete multi-byte
/// sequences at the end of a chunk are kept until the next chunk arrives;
/// invalid bytes are replaced with U+FFFD.
#[derive(Debug, Default)]
pub struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    pub fn decode(&mut self, bytes: &[u8]) -> String {
        let mut buffer = std::mem::take(&mut self.pending);
        buffer.extend_from_slice(bytes);
        let mut out = String::with_capacity(buffer.len());
        let mut rest: &[u8] = &buffer;
        loop {
            match std::str::from_utf8(rest) {
                Ok(text) => {
                    out.push_str(text);
                    break;
                }
                Err(err) => {
                    let (valid, after) = rest.split_at(err.valid_up_to());
                    // `valid_up_to` guarantees that this prefix is UTF-8.
                    out.push_str(std::str::from_utf8(valid).unwrap_or_default());
                    match err.error_len() {
                        Some(len) => {
                            out.push('\u{FFFD}');
                            rest = &after[len..];
                        }
                        None => {
                            self.pending = after.to_vec();
                            break;
                        }
                    }
                }
            }
        }
        out
    }
}

/// Splits decoded text into lines. Carriage returns are dropped; a line that
/// stays incomplete for longer than the idle timeout is emitted anyway so
/// output without a trailing newline (prompts, `print!`) becomes visible.
#[derive(Debug, Default)]
pub struct LineAssembler {
    partial: String,
    partial_since: Option<Instant>,
}

/// Upper bound for a single line, protecting against binary data without
/// line breaks.
const MAX_LINE_LEN: usize = 4096;

impl LineAssembler {
    pub fn push(&mut self, text: &str, now: Instant, out: &mut Vec<String>) {
        for ch in text.chars() {
            match ch {
                '\n' => {
                    out.push(std::mem::take(&mut self.partial));
                    self.partial_since = None;
                }
                '\r' => {}
                _ => {
                    if self.partial.is_empty() {
                        self.partial_since = Some(now);
                    }
                    self.partial.push(ch);
                    if self.partial.len() >= MAX_LINE_LEN {
                        out.push(std::mem::take(&mut self.partial));
                        self.partial_since = None;
                    }
                }
            }
        }
    }

    /// Emit the pending partial line if it has been idle for `idle`.
    pub fn flush_idle(&mut self, now: Instant, idle: Duration, out: &mut Vec<String>) {
        if let Some(since) = self.partial_since
            && now.duration_since(since) >= idle
            && !self.partial.is_empty()
        {
            out.push(std::mem::take(&mut self.partial));
            self.partial_since = None;
        }
    }

    pub fn take_partial(&mut self) -> Option<String> {
        self.partial_since = None;
        (!self.partial.is_empty()).then(|| std::mem::take(&mut self.partial))
    }
}

static RE_ADDRESS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"0[xX][[:xdigit:]]{8}").expect("valid regex"));

/// Lines written by the first-stage bootloader contain load addresses that
/// resolve to misleading symbols; espflash skips them as well.
const SUPPRESSED_PREFIXES: &[&str] = &["Saved PC:", "load:0x", "entry 0x"];

type Reader = EndianArcSlice<RunTimeEndian>;

/// Resolves code addresses to function names and source locations.
pub struct SymbolResolver {
    context: Context<Reader>,
    segments: Vec<(u64, u64)>,
    symbols: Vec<(u64, u64, String)>,
}

impl std::fmt::Debug for SymbolResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymbolResolver").field("symbols", &self.symbols.len()).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAddress {
    pub address: u64,
    pub function: String,
    pub location: Option<(String, u32)>,
}

impl SymbolResolver {
    pub fn from_elf(data: &[u8]) -> Result<Self> {
        let object = object::File::parse(data)
            .map_err(|e| Error::invalid_file("The ELF file could not be parsed.").with_detail(e.to_string()))?;
        let endian = if object.is_little_endian() { RunTimeEndian::Little } else { RunTimeEndian::Big };
        let dwarf = gimli::Dwarf::load(|id: gimli::SectionId| -> std::result::Result<Reader, gimli::Error> {
            let bytes: Arc<[u8]> = object
                .section_by_name(id.name())
                .and_then(|s| s.uncompressed_data().ok())
                .map(|d| Arc::from(d.as_ref()))
                .unwrap_or_else(|| Arc::from(&[][..]));
            Ok(EndianArcSlice::new(bytes, endian))
        })
        .map_err(|e| Error::invalid_file("The ELF debug information could not be loaded.").with_detail(e.to_string()))?;
        let context = Context::from_dwarf(dwarf)
            .map_err(|e| Error::invalid_file("The ELF debug information is invalid.").with_detail(e.to_string()))?;
        let segments = object
            .segments()
            .map(|s| (s.address(), s.address().saturating_add(s.size())))
            .filter(|(start, end)| end > start)
            .collect();
        let mut symbols: Vec<(u64, u64, String)> = object
            .symbols()
            .filter(|s| s.size() > 0 && s.kind() == object::SymbolKind::Text)
            .filter_map(|s| {
                let name = s.name().ok().filter(|n| !n.is_empty())?;
                Some((
                    s.address(),
                    s.address().saturating_add(s.size()),
                    addr2line::demangle_auto(name.into(), None).into_owned(),
                ))
            })
            .collect();
        symbols.sort_by_key(|s| s.0);
        Ok(Self { context, segments, symbols })
    }

    pub fn resolve(&self, address: u64) -> Option<ResolvedAddress> {
        if !self.segments.iter().any(|(start, end)| (*start..*end).contains(&address)) {
            return None;
        }
        let from_dwarf = self
            .context
            .find_frames(address)
            .skip_all_loads()
            .ok()
            .and_then(|mut frames| frames.next().ok().flatten())
            .and_then(|frame| frame.function.and_then(|f| f.demangle().ok().map(|n| n.into_owned())));
        let function = from_dwarf.or_else(|| {
            self.symbols
                .iter()
                .find(|(start, end, _)| (*start..*end).contains(&address))
                .map(|(_, _, name)| name.clone())
        })?;
        let location = self
            .context
            .find_location(address)
            .ok()
            .flatten()
            .and_then(|loc| Some((loc.file?.to_owned(), loc.line?)));
        Some(ResolvedAddress { address, function, location })
    }

    /// Resolve every code address in a line of output.
    pub fn resolve_line(&self, line: &str) -> Vec<ResolvedAddress> {
        let plain = strip_ansi(line);
        let trimmed = plain.trim_start();
        if SUPPRESSED_PREFIXES.iter().any(|p| trimmed.starts_with(p)) {
            return Vec::new();
        }
        let mut seen = Vec::new();
        RE_ADDRESS
            .find_iter(&plain)
            .filter_map(|m| u64::from_str_radix(&m.as_str()[2..], 16).ok())
            .filter(|addr| {
                if seen.contains(addr) {
                    false
                } else {
                    seen.push(*addr);
                    true
                }
            })
            .filter_map(|addr| self.resolve(addr))
            .collect()
    }
}

/// Remove ANSI escape sequences from a line.
pub fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn format_resolved(r: &ResolvedAddress) -> String {
    match &r.location {
        Some((file, line)) => format!("0x{:08x} - {} at {}:{}", r.address, r.function, file, line),
        None => format!("0x{:08x} - {}", r.address, r.function),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LineKind {
    /// Output received from the device.
    Output,
    /// Address resolved from the firmware ELF.
    Decoded,
    /// Data sent to the device by the user.
    Sent,
    /// Status message from OxidizeESP.
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MonitorLine {
    #[ts(type = "number")]
    pub seq: u64,
    /// Milliseconds since the Unix epoch when the line was completed.
    pub timestamp: f64,
    pub kind: LineKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "event", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum MonitorEvent {
    Started { monitor_id: String, port: String, baud: u32, symbols_loaded: bool },
    Lines { lines: Vec<MonitorLine> },
    /// The monitor ended. `error` is set when it ended because of a failure,
    /// for example after the device was unplugged.
    Stopped { error: Option<Error> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LineEnding {
    None,
    #[default]
    Lf,
    Cr,
    CrLf,
}

impl LineEnding {
    pub fn suffix(self) -> &'static str {
        match self {
            LineEnding::None => "",
            LineEnding::Lf => "\n",
            LineEnding::Cr => "\r",
            LineEnding::CrLf => "\r\n",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MonitorRequest {
    pub device_id: String,
    pub baud: u32,
    /// Firmware ELF used to resolve addresses.
    pub elf_path: Option<String>,
    /// Append received output to this file while monitoring.
    pub log_path: Option<String>,
    /// Reset the chip after opening the port so the boot log is captured.
    pub reset_on_start: bool,
}

/// Commands sent to a running monitor.
#[derive(Debug)]
pub enum MonitorCommand {
    Send(Vec<u8>),
    Reset,
    Stop,
}

pub trait MonitorSink: Send + Sync {
    fn emit(&self, event: MonitorEvent);
}

impl<F: Fn(MonitorEvent) + Send + Sync> MonitorSink for F {
    fn emit(&self, event: MonitorEvent) {
        self(event)
    }
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

/// Batches lines and forwards them to the sink and an optional log file.
struct Output<'a> {
    sink: &'a dyn MonitorSink,
    batch: Vec<MonitorLine>,
    seq: u64,
    last_flush: Instant,
    log: Option<BufWriter<File>>,
    log_failed: bool,
}

impl Output<'_> {
    fn push(&mut self, kind: LineKind, text: String) {
        let timestamp = now_ms();
        if let Some(log) = self.log.as_mut()
            && kind != LineKind::System
        {
            let prefix = match kind {
                LineKind::Sent => "> ",
                LineKind::Decoded => "  ",
                _ => "",
            };
            if writeln!(log, "{prefix}{}", strip_ansi(&text)).is_err() && !self.log_failed {
                self.log_failed = true;
                self.log = None;
                self.batch.push(MonitorLine {
                    seq: self.seq,
                    timestamp,
                    kind: LineKind::System,
                    text: "Writing to the log file failed; file logging was stopped.".into(),
                });
                self.seq += 1;
            }
        }
        self.batch.push(MonitorLine { seq: self.seq, timestamp, kind, text });
        self.seq += 1;
    }

    fn flush(&mut self, force: bool) {
        let due = self.last_flush.elapsed() >= Duration::from_millis(50) || self.batch.len() >= 500;
        if (force || due) && !self.batch.is_empty() {
            self.sink.emit(MonitorEvent::Lines { lines: std::mem::take(&mut self.batch) });
            self.last_flush = Instant::now();
            if let Some(log) = self.log.as_mut() {
                let _ = log.flush();
            }
        }
    }
}

pub struct MonitorConfig {
    pub monitor_id: String,
    pub port: String,
    pub baud: u32,
    pub symbols: Option<SymbolResolver>,
    pub log_path: Option<std::path::PathBuf>,
    pub reset_on_start: bool,
}

pub fn open_log(path: &Path) -> Result<BufWriter<File>> {
    fsutil::validate_output(path)?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(BufWriter::new)
        .map_err(|e| Error::file_io(&e, path, "open"))
}

/// Run the monitor until it is stopped or the connection fails. Returns
/// the error that ended the monitor, if any.
pub fn run(
    mut link: Box<dyn SerialLink>,
    config: MonitorConfig,
    sink: &dyn MonitorSink,
    commands: &mpsc::Receiver<MonitorCommand>,
) -> Option<Error> {
    let log = match config.log_path.as_deref().map(open_log).transpose() {
        Ok(log) => log,
        Err(err) => {
            sink.emit(MonitorEvent::Stopped { error: Some(err.clone()) });
            return Some(err);
        }
    };
    let mut out = Output { sink, batch: Vec::new(), seq: 0, last_flush: Instant::now(), log, log_failed: false };
    sink.emit(MonitorEvent::Started {
        monitor_id: config.monitor_id.clone(),
        port: config.port.clone(),
        baud: config.baud,
        symbols_loaded: config.symbols.is_some(),
    });
    if let Some(path) = &config.log_path {
        out.push(LineKind::System, format!("Logging to {}", display_name(path)));
    }

    let _ = link.set_timeout(Duration::from_millis(20));
    if config.reset_on_start {
        match link.reset() {
            Ok(()) => out.push(LineKind::System, "Device reset".into()),
            Err(err) => out.push(LineKind::System, format!("Reset failed: {}", err.message)),
        }
    }

    let mut decoder = Utf8Decoder::default();
    let mut lines = LineAssembler::default();
    let mut buf = vec![0u8; 4096];
    let mut completed = Vec::new();
    let failure = loop {
        match commands.try_recv() {
            Ok(MonitorCommand::Stop) | Err(mpsc::TryRecvError::Disconnected) => break None,
            Ok(MonitorCommand::Reset) => match link.reset() {
                Ok(()) => out.push(LineKind::System, "Device reset".into()),
                Err(err) => out.push(LineKind::System, format!("Reset failed: {}", err.message)),
            },
            Ok(MonitorCommand::Send(data)) => {
                let shown = String::from_utf8_lossy(&data).trim_end_matches(['\r', '\n']).to_owned();
                match link.write_all(&data) {
                    Ok(()) => out.push(LineKind::Sent, shown),
                    Err(err) => break Some(err),
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }

        match link.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                let text = decoder.decode(&buf[..n]);
                lines.push(&text, Instant::now(), &mut completed);
            }
            Err(err) => break Some(err),
        }
        lines.flush_idle(Instant::now(), Duration::from_millis(150), &mut completed);
        for line in completed.drain(..) {
            let resolved = config.symbols.as_ref().map(|s| s.resolve_line(&line)).unwrap_or_default();
            out.push(LineKind::Output, line);
            for r in resolved {
                out.push(LineKind::Decoded, format_resolved(&r));
            }
        }
        out.flush(false);
    };

    if let Some(partial) = lines.take_partial() {
        out.push(LineKind::Output, partial);
    }
    if let Some(err) = &failure {
        out.push(LineKind::System, err.message.clone());
    }
    out.flush(true);
    sink.emit(MonitorEvent::Stopped { error: failure.clone() });
    failure
}

/// Validate and encode user input for transmission.
pub fn encode_input(text: &str, ending: LineEnding) -> Result<Vec<u8>> {
    if text.len() > 4096 {
        return Err(Error::new(ErrorKind::InvalidInput, "The text to send is too long (maximum 4096 bytes)."));
    }
    let mut data = text.as_bytes().to_vec();
    data.extend_from_slice(ending.suffix().as_bytes());
    if data.is_empty() {
        return Err(Error::invalid_input("Nothing to send."));
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{
        hardware::{
            Backend,
            mock::{MockBackend, SimDevice},
        },
        model::{Chip, FlashSize},
    };

    #[test]
    fn decoder_handles_split_multibyte_sequences() {
        let mut d = Utf8Decoder::default();
        let bytes = "grüße 🙈".as_bytes();
        let mut out = String::new();
        for b in bytes {
            out.push_str(&d.decode(std::slice::from_ref(b)));
        }
        assert_eq!(out, "grüße 🙈");
    }

    #[test]
    fn decoder_replaces_invalid_bytes() {
        let mut d = Utf8Decoder::default();
        assert_eq!(d.decode(b"a\xFFb"), "a\u{FFFD}b");
    }

    #[test]
    fn assembler_splits_lines_and_flushes_idle_partials() {
        let mut a = LineAssembler::default();
        let mut out = Vec::new();
        let t0 = Instant::now();
        a.push("one\r\ntwo\nthr", t0, &mut out);
        assert_eq!(out, vec!["one", "two"]);
        a.flush_idle(t0, Duration::from_millis(100), &mut out);
        assert_eq!(out.len(), 2);
        a.flush_idle(t0 + Duration::from_millis(200), Duration::from_millis(100), &mut out);
        assert_eq!(out, vec!["one", "two", "thr"]);
    }

    #[test]
    fn assembler_caps_line_length() {
        let mut a = LineAssembler::default();
        let mut out = Vec::new();
        a.push(&"x".repeat(MAX_LINE_LEN + 10), Instant::now(), &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), MAX_LINE_LEN);
        assert_eq!(a.take_partial().map(|p| p.len()), Some(10));
    }

    #[test]
    fn strips_ansi_sequences() {
        assert_eq!(strip_ansi("\x1b[0;32mI (10) app: hi\x1b[0m"), "I (10) app: hi");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn encodes_input_with_line_endings() {
        assert_eq!(encode_input("help", LineEnding::CrLf).unwrap(), b"help\r\n");
        assert_eq!(encode_input("x", LineEnding::None).unwrap(), b"x");
        assert!(encode_input("", LineEnding::None).is_err());
        assert!(encode_input(&"a".repeat(5000), LineEnding::Lf).is_err());
    }

    #[test]
    fn rejects_invalid_elf_for_symbols() {
        assert!(SymbolResolver::from_elf(b"not an elf").is_err());
    }

    #[derive(Default)]
    struct Collect(Mutex<Vec<MonitorEvent>>);
    impl MonitorSink for Collect {
        fn emit(&self, event: MonitorEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn monitor_streams_lines_and_stops_on_unplug() {
        let backend = MockBackend::new();
        let device = backend.add(SimDevice::new("SIM", Chip::Esp32c3, FlashSize::Mb4, "S1"));
        let port = backend.list_ports().unwrap().remove(0);
        let link = backend.open_serial(&port, 115_200).unwrap();
        let sink = Arc::new(Collect::default());
        let (tx, rx) = mpsc::channel();
        let sink2 = sink.clone();
        let handle = std::thread::spawn(move || {
            run(
                link,
                MonitorConfig {
                    monitor_id: "m".into(),
                    port: "SIM".into(),
                    baud: 115_200,
                    symbols: None,
                    log_path: None,
                    reset_on_start: false,
                },
                sink2.as_ref(),
                &rx,
            )
        });
        tx.send(MonitorCommand::Send(b"hello\n".to_vec())).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        device.lock().unwrap().plugged_in = false;
        let failure = handle.join().unwrap();
        assert_eq!(failure.map(|e| e.kind), Some(ErrorKind::Disconnected));
        assert_eq!(device.lock().unwrap().sent, b"hello\n");

        let events = sink.0.lock().unwrap();
        assert!(matches!(events.first(), Some(MonitorEvent::Started { .. })));
        let lines: Vec<&MonitorLine> = events
            .iter()
            .filter_map(|e| match e {
                MonitorEvent::Lines { lines } => Some(lines.iter()),
                _ => None,
            })
            .flatten()
            .collect();
        assert!(lines.iter().any(|l| l.kind == LineKind::Output && l.text.starts_with("ESP-ROM")));
        assert!(lines.iter().any(|l| l.kind == LineKind::Sent && l.text == "hello"));
        assert!(lines.windows(2).all(|w| w[0].seq < w[1].seq));
        assert!(matches!(events.last(), Some(MonitorEvent::Stopped { error: Some(_) })));
    }
}
