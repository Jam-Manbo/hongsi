use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{SubmissionInfo, SubmissionState};

pub const JOB_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    #[default]
    Prepare,
    Transfer,
    Upload,
    Submit,
    Verify,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: Stage,
    pub file_name: Option<String>,
    pub file_count: usize,
    pub uploaded_files: usize,
    pub sent_bytes: u64,
    pub total_bytes: u64,
}

impl Progress {
    pub fn at(stage: Stage) -> Self { Self { stage, ..Self::default() } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status { Running, Complete, Failed, Uncertain }

#[derive(Clone, Debug, Serialize)]
pub struct Failure {
    pub status: u16,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub revision: u64,
    pub status: Status,
    pub progress: Progress,
    pub result: Option<Value>,
    pub error: Option<Failure>,
}

pub type Observer = Arc<dyn Fn(Snapshot) + Send + Sync>;

struct Expected {
    files: Vec<(String, i64)>,
    before_status: SubmissionState,
    before_modified: Option<i64>,
    acknowledged: bool,
}

pub struct Job {
    owner: String,
    cmid: i64,
    created: Instant,
    state: Mutex<Snapshot>,
    expected: Mutex<Option<Expected>>,
    observer: Mutex<Option<Observer>>,
}

impl Job {
    pub fn guard(self: &Arc<Self>) -> JobGuard { JobGuard(self.clone()) }

    pub fn snapshot(&self) -> Snapshot { self.state.lock().expect("제출 상태 잠금").clone() }

    fn change(&self, change: impl FnOnce(&mut Snapshot)) {
        let snapshot = {
            let mut state = self.state.lock().expect("제출 상태 잠금");
            change(&mut state);
            state.revision += 1;
            state.clone()
        };
        let observer = self.observer.lock().expect("제출 알림 잠금").clone();
        if let Some(observer) = observer { observer(snapshot); }
    }

    pub fn observe(&self, observer: Observer) {
        *self.observer.lock().expect("제출 알림 잠금") = Some(observer.clone());
        observer(self.snapshot());
    }

    pub fn progress(&self, progress: Progress) {
        self.change(|state| { state.progress = progress; });
    }

    pub fn expect(&self, before: &SubmissionInfo, files: &[(String, Vec<u8>)]) {
        let mut files: Vec<_> = files.iter().map(|(name, bytes)| (name.clone(), bytes.len() as i64)).collect();
        files.sort();
        *self.expected.lock().expect("제출 파일 잠금") = Some(Expected {
            files, before_status: before.status, before_modified: before.modified, acknowledged: false,
        });
    }

    pub fn acknowledge(&self) {
        if let Some(expected) = self.expected.lock().expect("제출 파일 잠금").as_mut() { expected.acknowledged = true; }
    }

    pub fn matches(&self, info: &SubmissionInfo) -> bool {
        let expected = self.expected.lock().expect("제출 파일 잠금");
        let Some(expected) = expected.as_ref() else { return false };
        if info.status != SubmissionState::Submitted { return false; }
        let mut files: Vec<_> = info.files.iter().map(|file| (file.name.clone(), file.size.unwrap_or(-1))).collect();
        files.sort();
        files == expected.files && (expected.acknowledged || expected.before_status != SubmissionState::Submitted
            || info.modified.is_some_and(|modified| expected.before_modified.is_none_or(|before| modified > before)))
    }

    pub fn complete(&self, result: Value) {
        self.change(|state| {
            state.status = Status::Complete;
            state.progress = Progress::at(Stage::Verify);
            state.result = Some(result);
            state.error = None;
        });
        self.observer.lock().expect("제출 알림 잠금").take();
    }

    pub fn fail(&self, status: u16, code: impl Into<String>, message: impl Into<String>) {
        self.change(|state| {
            state.status = if matches!(state.progress.stage, Stage::Submit | Stage::Verify) { Status::Uncertain } else { Status::Failed };
            state.error = Some(Failure { status, code: code.into(), message: message.into() });
        });
        self.observer.lock().expect("제출 알림 잠금").take();
    }
}

pub struct JobGuard(Arc<Job>);

impl Drop for JobGuard {
    fn drop(&mut self) {
        if self.0.snapshot().status == Status::Running {
            self.0.fail(503, "submission_interrupted", "제출 처리와의 연결이 끊겼어요. 제출 상태를 확인해 주세요.");
        }
    }
}

#[derive(Default)]
pub struct Jobs(Mutex<HashMap<String, Arc<Job>>>);

impl Jobs {
    pub fn start(&self, owner: &str, cmid: i64, id: &str) -> Result<(Arc<Job>, bool), &'static str> {
        if id.len() != 32 || !id.bytes().all(|c| c.is_ascii_hexdigit()) || cmid <= 0 { return Err("잘못된 제출 요청이에요."); }
        let mut jobs = self.0.lock().expect("제출 목록 잠금");
        jobs.retain(|_, job| job.created.elapsed() < Duration::from_secs(1800));
        if let Some(job) = jobs.get(id) {
            return if job.owner == owner && job.cmid == cmid { Ok((job.clone(), false)) } else { Err("잘못된 제출 요청이에요.") };
        }
        if jobs.values().any(|job| job.owner == owner && job.cmid == cmid && job.snapshot().status == Status::Running) {
            return Err("이 과제를 제출하고 있어요. 제출 상태를 먼저 확인해 주세요.");
        }
        if jobs.len() >= 512 { return Err("제출 요청이 많아요. 잠시 후 다시 시도해 주세요."); }
        let job = Arc::new(Job {
            owner: owner.to_owned(), cmid, created: Instant::now(),
            state: Mutex::new(Snapshot { id: id.to_owned(), revision: 0, status: Status::Running, progress: Progress::default(), result: None, error: None }),
            expected: Mutex::new(None), observer: Mutex::new(None),
        });
        jobs.insert(id.to_owned(), job.clone());
        Ok((job, true))
    }

    pub fn get(&self, owner: &str, cmid: i64, id: &str) -> Option<Arc<Job>> {
        self.0.lock().expect("제출 목록 잠금").get(id)
            .filter(|job| job.owner == owner && job.cmid == cmid && job.created.elapsed() < Duration::from_secs(1800)).cloned()
    }

    pub fn clear(&self) { self.0.lock().expect("제출 목록 잠금").clear(); }
}
