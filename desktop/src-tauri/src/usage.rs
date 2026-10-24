//! Hard safety net for the Google engine: a persisted daily request counter,
//! checked and incremented *before* every call to the Gemini API, so a bug
//! anywhere upstream (a detection loop, a runaway page, a client retry)
//! cannot spend past a cap the user set without Google-side billing alerts
//! being involved at all.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

use crate::engines::EngineError;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct UsageRecord {
    /// Days since the Unix epoch (UTC) -- avoids pulling in a date/time
    /// crate just to get daily rollover right.
    pub day_bucket: u64,
    pub count: u32,
}

pub struct UsageTracker {
    path: PathBuf,
    state: Mutex<UsageRecord>,
}

fn current_day_bucket() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() / 86_400
}

fn usage_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("app data dir");
    std::fs::create_dir_all(&dir).ok();
    dir.join("google_usage.json")
}

impl UsageTracker {
    pub fn load(app: &tauri::AppHandle) -> Self {
        let path = usage_path(app);
        let mut record: UsageRecord = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let today = current_day_bucket();
        if record.day_bucket != today {
            record = UsageRecord { day_bucket: today, count: 0 };
        }
        Self { path, state: Mutex::new(record) }
    }

    fn persist(&self, record: UsageRecord) {
        if let Ok(data) = serde_json::to_string(&record) {
            let _ = std::fs::write(&self.path, data);
        }
    }

    /// Returns the current count/cap for display, rolling the day over if needed.
    pub fn snapshot(&self, cap: u32) -> (u32, u32) {
        let mut guard = self.state.lock().unwrap();
        let today = current_day_bucket();
        if guard.day_bucket != today {
            *guard = UsageRecord { day_bucket: today, count: 0 };
            self.persist(*guard);
        }
        (guard.count, cap)
    }

    /// Call right before every Google API request. cap == 0 means unlimited.
    /// Fails closed: on success the count is already incremented, so a
    /// request that's about to be attempted is never left uncounted.
    pub fn check_and_record(&self, cap: u32) -> Result<(), EngineError> {
        let mut guard = self.state.lock().unwrap();
        let today = current_day_bucket();
        if guard.day_bucket != today {
            *guard = UsageRecord { day_bucket: today, count: 0 };
        }
        if cap > 0 && guard.count >= cap {
            return Err(EngineError::new(
                "GOOGLE_DAILY_CAP_REACHED",
                format!(
                    "Daily Google API safety cap reached ({}/{cap} requests today). \
                     Raise \"Google daily request cap\" in settings, switch to the local \
                     engine, or wait until tomorrow (UTC).",
                    guard.count
                ),
            ));
        }
        guard.count += 1;
        self.persist(*guard);
        Ok(())
    }
}
