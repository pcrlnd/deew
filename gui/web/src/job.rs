//! The one deew process of the server, its replay buffer and the fan-out of
//! its output to all connected sockets.

use axum::body::Bytes;
use deew_gui_core::cli;
use deew_gui_core::pty_session::{self, PtySession, SpawnConfig};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::mpsc;

pub const REPLAY_CAPACITY: usize = 1024 * 1024;
/// Stored event lines other than `progress` ones (the oldest are dropped).
pub const EVENT_LOG_CAPACITY: usize = 1000;
/// Messages a socket may lag behind before it is disconnected.
pub const SUBSCRIBER_QUEUE: usize = 256;

/// Keeps the last `cap` bytes that were pushed.
pub struct RingBuffer {
    cap: usize,
    data: VecDeque<u8>,
}

impl RingBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            data: VecDeque::new(),
        }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        if bytes.len() >= self.cap {
            self.data.clear();
            self.data.extend(&bytes[bytes.len() - self.cap..]);
            return;
        }
        let overflow = (self.data.len() + bytes.len()).saturating_sub(self.cap);
        self.data.drain(..overflow);
        self.data.extend(bytes);
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn snapshot(&self) -> Vec<u8> {
        let (a, b) = self.data.as_slices();
        [a, b].concat()
    }
}

/// The event lines of the CLI (see `Event::Progress`) kept for clients that
/// connect later. Only the newest `progress` line of each job is kept, in the
/// place of the first one; the other lines are kept in order, up to
/// `EVENT_LOG_CAPACITY` of them.
#[derive(Default)]
pub struct EventLog {
    /// `Some(job)` for `progress` lines.
    lines: Vec<(Option<u64>, String)>,
    other: usize,
}

impl EventLog {
    /// Only `type` and `job` are read; a line that is not a JSON object with
    /// them is a plain line.
    fn progress_job(line: &str) -> Option<u64> {
        #[derive(serde::Deserialize)]
        struct Head {
            r#type: String,
            job: u64,
        }
        let head: Head = serde_json::from_str(line).ok()?;
        (head.r#type == "progress").then_some(head.job)
    }

    pub fn push(&mut self, line: &str) {
        match Self::progress_job(line) {
            Some(job) => match self.lines.iter_mut().find(|(j, _)| *j == Some(job)) {
                Some(entry) => entry.1 = line.to_string(),
                None => self.lines.push((Some(job), line.to_string())),
            },
            None => {
                self.lines.push((None, line.to_string()));
                self.other += 1;
                if self.other > EVENT_LOG_CAPACITY
                    && let Some(oldest) = self.lines.iter().position(|(j, _)| j.is_none())
                {
                    self.lines.remove(oldest);
                    self.other -= 1;
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.other = 0;
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.lines.iter().map(|(_, line)| line.clone()).collect()
    }
}

/// What subscribers receive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Output(Bytes),
    /// One raw event line (JSON) written by the CLI.
    Progress(String),
    Started(Vec<String>),
    Exit(Option<i32>),
}

struct Subscriber {
    id: u64,
    tx: mpsc::Sender<Event>,
}

struct Inner {
    /// Id of the current (or last) job; callbacks of older jobs are ignored.
    job_id: u64,
    session: Option<PtySession>,
    running: bool,
    args: Option<Vec<String>>,
    buffer: RingBuffer,
    events: EventLog,
    subscribers: Vec<Subscriber>,
}

impl Inner {
    /// Never blocks: a subscriber whose queue is full (or closed) is dropped.
    fn broadcast(&mut self, event: Event) {
        self.subscribers
            .retain(|s| s.tx.try_send(event.clone()).is_ok());
    }
}

fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(|e| e.into_inner())
}

/// State to send to a newly attached socket, plus its event stream.
pub struct Subscription {
    pub id: u64,
    pub running: bool,
    pub args: Option<Vec<String>>,
    /// Buffered output so far.
    pub replay: Vec<u8>,
    /// Stored event lines so far (see [`EventLog`]), sent before `replay`.
    pub progress: Vec<String>,
    pub events: mpsc::Receiver<Event>,
}

pub struct Job {
    inner: Arc<Mutex<Inner>>,
    /// Program to run; `None` means the deew CLI next to this executable.
    program: Option<PathBuf>,
    next_id: AtomicU64,
}

impl Job {
    pub fn new(program: Option<PathBuf>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                job_id: 0,
                session: None,
                running: false,
                args: None,
                buffer: RingBuffer::new(REPLAY_CAPACITY),
                events: EventLog::default(),
                subscribers: Vec::new(),
            })),
            program,
            next_id: AtomicU64::new(1),
        }
    }

    pub fn is_running(&self) -> bool {
        lock(&self.inner).running
    }

    /// Takes the buffer snapshot and subscribes under one lock, so no byte is
    /// lost or duplicated between the two.
    pub fn subscribe(&self) -> Subscription {
        let (tx, events) = mpsc::channel(SUBSCRIBER_QUEUE);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut inner = lock(&self.inner);
        inner.subscribers.push(Subscriber { id, tx });
        Subscription {
            id,
            running: inner.running,
            args: inner.args.clone(),
            // Only a running job is replayed; a finished one is just history.
            replay: if inner.running {
                inner.buffer.snapshot()
            } else {
                Vec::new()
            },
            progress: if inner.running {
                inner.events.snapshot()
            } else {
                Vec::new()
            },
            events,
        }
    }

    pub fn unsubscribe(&self, id: u64) {
        lock(&self.inner).subscribers.retain(|s| s.id != id);
    }

    /// `native` also reports the structured events of the CLI.
    pub fn start(
        &self,
        args: Vec<String>,
        cols: u16,
        rows: u16,
        native: bool,
    ) -> Result<(), String> {
        let program = match &self.program {
            Some(program) => program.clone(),
            None => cli::sidecar_path()?,
        };
        let mut inner = lock(&self.inner);
        if inner.running {
            return Err("deew is already running".into());
        }
        let job_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let config = SpawnConfig {
            program,
            args: args.clone(),
            cols,
            rows,
            cwd: std::env::home_dir(),
            env: vec![("COLORTERM".into(), "truecolor".into())],
            events: native,
        };
        let out_inner = Arc::clone(&self.inner);
        let event_inner = Arc::clone(&self.inner);
        let exit_inner = Arc::clone(&self.inner);
        // The callbacks run on other threads and wait for this lock, so they
        // cannot observe the state before it is complete.
        let session = pty_session::spawn(
            config,
            move |bytes| {
                let mut inner = lock(&out_inner);
                if inner.job_id == job_id {
                    inner.buffer.push(bytes);
                    inner.broadcast(Event::Output(Bytes::copy_from_slice(bytes)));
                }
            },
            move |line| {
                let mut inner = lock(&event_inner);
                if inner.job_id == job_id {
                    inner.events.push(line);
                    inner.broadcast(Event::Progress(line.to_string()));
                }
            },
            move |code| {
                let mut inner = lock(&exit_inner);
                if inner.job_id == job_id {
                    inner.session = None;
                    inner.running = false;
                    inner.broadcast(Event::Exit(Some(code)));
                }
            },
        )?;
        inner.job_id = job_id;
        inner.session = Some(session);
        inner.running = true;
        inner.args = Some(args.clone());
        inner.buffer.clear();
        inner.events.clear();
        inner.broadcast(Event::Started(args));
        Ok(())
    }

    pub fn write(&self, data: &[u8]) -> Result<(), String> {
        match lock(&self.inner).session.as_mut() {
            Some(session) => session.write(data),
            None => Ok(()),
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        match lock(&self.inner).session.as_ref() {
            Some(session) => session.resize(cols, rows),
            None => Ok(()),
        }
    }

    /// Ctrl+C, then kill after a grace period (see `PtySession::stop`).
    pub fn stop(&self) {
        if let Some(session) = lock(&self.inner).session.as_mut() {
            session.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_keeps_the_tail() {
        let mut b = RingBuffer::new(5);
        assert!(b.is_empty());
        b.push(b"ab");
        b.push(b"cd");
        assert_eq!(b.snapshot(), b"abcd");
        b.push(b"efg");
        assert_eq!(b.snapshot(), b"cdefg");
        assert_eq!(b.len(), 5);
        b.push(b"0123456789");
        assert_eq!(b.snapshot(), b"56789");
        b.push(b"");
        assert_eq!(b.snapshot(), b"56789");
        b.clear();
        assert!(b.is_empty());
        b.push(b"x");
        assert_eq!(b.snapshot(), b"x");
    }

    #[test]
    fn ring_buffer_wraps_many_times() {
        let mut b = RingBuffer::new(7);
        let mut all: Vec<u8> = Vec::new();
        for i in 0..100u8 {
            let chunk = vec![i; (i as usize % 4) + 1];
            all.extend(&chunk);
            b.push(&chunk);
        }
        assert_eq!(b.snapshot(), all[all.len() - 7..]);
    }

    fn progress(job: u64, percent: u32) -> String {
        format!(r#"{{"type":"progress","job":{job},"stage":"encode","percent":{percent}}}"#)
    }

    #[test]
    fn event_log_keeps_the_newest_progress_per_job_in_place() {
        let mut log = EventLog::default();
        log.push(r#"{"type":"hello","version":"1"}"#);
        log.push(&progress(0, 1));
        log.push(&progress(1, 2));
        log.push(r#"{"type":"warning","text":"w"}"#);
        log.push(&progress(0, 50));
        log.push(&progress(1, 60));
        log.push(&progress(0, 99));
        assert_eq!(
            log.snapshot(),
            [
                r#"{"type":"hello","version":"1"}"#.to_string(),
                progress(0, 99),
                progress(1, 60),
                r#"{"type":"warning","text":"w"}"#.to_string(),
            ]
        );
    }

    #[test]
    fn event_log_treats_unparsable_lines_as_plain() {
        let mut log = EventLog::default();
        log.push("not json");
        log.push("not json");
        // A progress line without a job cannot be replaced.
        log.push(r#"{"type":"progress"}"#);
        log.push(r#"{"type":"progress","job":"x"}"#);
        assert_eq!(log.snapshot().len(), 4);
    }

    #[test]
    fn event_log_caps_other_lines_and_keeps_progress() {
        let mut log = EventLog::default();
        log.push(&progress(0, 5));
        for i in 0..EVENT_LOG_CAPACITY + 20 {
            log.push(&format!(r#"{{"type":"warning","text":"{i}"}}"#));
        }
        log.push(&progress(0, 6));
        let lines = log.snapshot();
        assert_eq!(lines.len(), EVENT_LOG_CAPACITY + 1);
        assert_eq!(lines[0], progress(0, 6));
        assert_eq!(lines[1], r#"{"type":"warning","text":"20"}"#);
        assert_eq!(
            lines.last().unwrap(),
            &format!(
                r#"{{"type":"warning","text":"{}"}}"#,
                EVENT_LOG_CAPACITY + 19
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn event_log_is_cleared_on_start() {
        let job = Job::new(Some(PathBuf::from("/bin/sh")));
        lock(&job.inner).events.push(&progress(0, 1));
        job.start(vec!["-c".into(), "true".into()], 80, 24, false)
            .unwrap();
        assert!(lock(&job.inner).events.snapshot().is_empty());
        assert!(job.subscribe().progress.is_empty());
    }

    #[test]
    fn slow_subscriber_is_dropped_without_blocking() {
        let job = Job::new(None);
        let mut slow = job.subscribe();
        let mut fast = job.subscribe();
        let fast_id = fast.id;
        {
            let mut inner = lock(&job.inner);
            for i in 0..(SUBSCRIBER_QUEUE + 10) {
                inner.broadcast(Event::Output(Bytes::from(vec![i as u8])));
                // keep `fast` drained
                while fast.events.try_recv().is_ok() {}
            }
            assert_eq!(inner.subscribers.len(), 1);
            assert_eq!(inner.subscribers[0].id, fast_id);
        }
        let mut n = 0;
        while slow.events.try_recv().is_ok() {
            n += 1;
        }
        assert_eq!(n, SUBSCRIBER_QUEUE);
    }
}
