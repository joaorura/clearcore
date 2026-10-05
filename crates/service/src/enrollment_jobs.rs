// removed when Task S6 wires the module
#![allow(dead_code)]
//! Thread-backed enrollment job table (task S5, spec 4.3).
//!
//! The table itself lives on the daemon thread; only the result channel
//! crosses threads.

use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, Sender};

const MAX_FINISHED_JOBS: usize = 64;
const PANIC_ERROR_CODE: &str = "ENROLL_FAILED";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone)]
pub struct JobInfo {
    pub job_id: String,
    pub state: JobState,
    pub stage: &'static str,
    pub error_code: Option<&'static str>,
    pub remaining_seconds: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
pub struct JobFailure {
    pub error_code: &'static str,
    pub stage: &'static str,
    pub remaining_seconds: Option<f32>,
}

pub struct Completion<T> {
    pub job_id: String,
    pub result: Result<T, JobFailure>,
}

pub struct JobTable<T: Send + 'static> {
    next_id: u64,
    jobs: HashMap<String, JobInfo>,
    finished: VecDeque<String>,
    rx: Receiver<Completion<T>>,
    tx: Sender<Completion<T>>,
}

impl<T: Send + 'static> Default for JobTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + 'static> JobTable<T> {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            next_id: 1,
            jobs: HashMap::new(),
            finished: VecDeque::new(),
            rx,
            tx,
        }
    }

    /// Registers a Running job "job-<n>" and runs `work` on a new thread; the
    /// result is queued for `drain`.
    pub fn spawn<F>(&mut self, stage: &'static str, work: F) -> String
    where
        F: FnOnce() -> Result<T, JobFailure> + Send + 'static,
    {
        let job_id = format!("job-{}", self.next_id);
        self.next_id += 1;
        self.jobs.insert(
            job_id.clone(),
            JobInfo {
                job_id: job_id.clone(),
                state: JobState::Running,
                stage,
                error_code: None,
                remaining_seconds: None,
            },
        );

        let tx = self.tx.clone();
        let thread_id = job_id.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("enroll-{job_id}"))
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(work)).unwrap_or(Err(JobFailure {
                    error_code: PANIC_ERROR_CODE,
                    stage,
                    remaining_seconds: None,
                }));
                // The table may be gone already; nobody is left to hear the result.
                let _ = tx.send(Completion {
                    job_id: thread_id,
                    result,
                });
            });
        if spawned.is_err() {
            self.mark_failed(
                &job_id,
                JobFailure {
                    error_code: PANIC_ERROR_CODE,
                    stage,
                    remaining_seconds: None,
                },
            );
        }
        job_id
    }

    /// Moves every finished job to its final state and returns the successful
    /// payloads in completion order.
    pub fn drain(&mut self) -> Vec<(String, T)> {
        let mut payloads = Vec::new();
        while let Ok(done) = self.rx.try_recv() {
            match done.result {
                Ok(payload) => {
                    self.mark_done(&done.job_id);
                    payloads.push((done.job_id, payload));
                }
                Err(failure) => self.mark_failed(&done.job_id, failure),
            }
        }
        payloads
    }

    /// For failures found while applying a drained result.
    pub fn mark_failed(&mut self, job_id: &str, failure: JobFailure) {
        if let Some(info) = self.jobs.get_mut(job_id) {
            info.state = JobState::Failed;
            info.stage = failure.stage;
            info.error_code = Some(failure.error_code);
            info.remaining_seconds = failure.remaining_seconds;
            self.record_finished(job_id);
        }
    }

    pub fn mark_done(&mut self, job_id: &str) {
        if let Some(info) = self.jobs.get_mut(job_id) {
            info.state = JobState::Done;
            info.error_code = None;
            self.record_finished(job_id);
        }
    }

    pub fn info(&self, job_id: &str) -> Option<&JobInfo> {
        self.jobs.get(job_id)
    }

    fn record_finished(&mut self, job_id: &str) {
        if !self.finished.iter().any(|id| id == job_id) {
            self.finished.push_back(job_id.to_owned());
        }
        while self.finished.len() > MAX_FINISHED_JOBS {
            if let Some(oldest) = self.finished.pop_front() {
                self.jobs.remove(&oldest);
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::redundant_clone
)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_until<R>(mut probe: impl FnMut() -> Option<R>) -> R {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(found) = probe() {
                return found;
            }
            assert!(Instant::now() < deadline, "wait_until timed out after 5 s");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn failure(stage: &'static str) -> JobFailure {
        JobFailure {
            error_code: "ENROLL_FAILED",
            stage,
            remaining_seconds: None,
        }
    }

    #[test]
    fn spawned_job_completes_and_drain_returns_its_payload() {
        let mut t = JobTable::<u32>::new();
        let id = t.spawn("denoise", || Ok(7));
        let got = wait_until(|| {
            let d = t.drain();
            if d.is_empty() { None } else { Some(d) }
        });
        assert_eq!(got, vec![(id.clone(), 7)]);
    }

    #[test]
    fn failing_job_is_reported_with_its_code_and_not_returned() {
        let mut t = JobTable::<u32>::new();
        let id = t.spawn("enroll", || Err(failure("enroll")));
        let drained = wait_until(|| {
            let d = t.drain();
            (t.info(&id).is_some_and(|i| i.state == JobState::Failed)).then_some(d)
        });
        assert!(drained.is_empty());
        let info = t.info(&id).unwrap();
        assert_eq!(info.error_code, Some("ENROLL_FAILED"));
        assert_eq!(info.stage, "enroll");
    }

    #[test]
    fn job_ids_are_unique_and_unknown_id_is_none() {
        let mut t = JobTable::<u32>::new();
        let a = t.spawn("s", || Ok(1));
        let b = t.spawn("s", || Ok(2));
        assert_ne!(a, b);
        assert_eq!(a, "job-1");
        assert_eq!(b, "job-2");
        assert!(t.info("nope").is_none());
    }

    #[test]
    fn only_the_last_64_finished_jobs_are_kept() {
        let mut t = JobTable::<u32>::new();
        let mut ids = Vec::new();
        for n in 0..70 {
            ids.push(t.spawn("s", move || Ok(n)));
            wait_until(|| (!t.drain().is_empty()).then_some(()));
        }
        assert!(t.info(&ids[0]).is_none());
        assert!(t.info(&ids[5]).is_none());
        assert!(t.info(&ids[6]).is_some());
        assert_eq!(t.info(&ids[69]).unwrap().state, JobState::Done);
    }

    #[test]
    fn panicking_work_becomes_a_failed_job_and_the_table_keeps_working() {
        let mut t = JobTable::<u32>::new();
        let bad = t.spawn("enroll", || panic!("boom"));
        wait_until(|| {
            t.drain();
            t.info(&bad)
                .is_some_and(|i| i.state == JobState::Failed)
                .then_some(())
        });
        let info = t.info(&bad).unwrap();
        assert_eq!(info.error_code, Some("ENROLL_FAILED"));
        assert_eq!(info.stage, "enroll");
        let good = t.spawn("denoise", || Ok(1));
        let got = wait_until(|| {
            let d = t.drain();
            if d.is_empty() { None } else { Some(d) }
        });
        assert_eq!(got, vec![(good, 1)]);
    }

    #[test]
    fn drain_returns_payloads_in_completion_order() {
        let mut t = JobTable::<u32>::new();
        let slow = t.spawn("s", || {
            std::thread::sleep(Duration::from_millis(150));
            Ok(1)
        });
        let fast = t.spawn("s", || Ok(2));
        let mut got = Vec::new();
        wait_until(|| {
            got.extend(t.drain());
            (got.len() == 2).then_some(())
        });
        assert_eq!(got, vec![(fast, 2), (slow, 1)]);
    }

    #[test]
    fn mark_failed_and_mark_done_update_state() {
        let mut t = JobTable::<u32>::new();
        let a = t.spawn("s", || Ok(1));
        let b = t.spawn("s", || Ok(2));
        let mut seen = 0;
        wait_until(|| {
            seen += t.drain().len();
            (seen == 2).then_some(())
        });
        t.mark_failed(
            &a,
            JobFailure {
                error_code: "X",
                stage: "apply",
                remaining_seconds: Some(1.5),
            },
        );
        let info = t.info(&a).unwrap();
        assert_eq!(info.state, JobState::Failed);
        assert_eq!(info.error_code, Some("X"));
        assert_eq!(info.stage, "apply");
        assert_eq!(info.remaining_seconds, Some(1.5));
        t.mark_done(&b);
        assert_eq!(t.info(&b).unwrap().state, JobState::Done);
    }
}
