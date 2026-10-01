use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::ApiError;

const WINDOW: Duration = Duration::from_secs(60);
const LIMIT: usize = 5;

#[derive(Default)]
pub struct LoginAttempts {
    accounts: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl LoginAttempts {
    pub fn check(&self, account: &str) -> Result<(), ApiError> {
        self.check_at(account, Instant::now())
    }

    fn check_at(&self, account: &str, now: Instant) -> Result<(), ApiError> {
        let mut accounts = self.accounts.lock().expect("로그인 요청 잠금");
        let attempts = accounts.entry(account.to_string()).or_default();
        while attempts.front().is_some_and(|at| now.duration_since(*at) >= WINDOW) {
            attempts.pop_front();
        }
        if attempts.len() >= LIMIT {
            let remaining = WINDOW.saturating_sub(now.duration_since(attempts[0]));
            let seconds = remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0);
            return Err(ApiError::rate_limited(seconds.max(1)));
        }
        attempts.push_back(now);
        Ok(())
    }

    pub fn sweep(&self) {
        let now = Instant::now();
        self.accounts.lock().expect("로그인 요청 잠금")
            .retain(|_, attempts| attempts.back().is_some_and(|at| now.duration_since(*at) < WINDOW));
    }
}
