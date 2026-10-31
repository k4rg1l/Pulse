//! The only component that starts a source. Each source gets its own task, so a
//! slow or failing one cannot delay the others.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use tokio::{sync::Notify, time};

use crate::{
    secrets,
    sources::{Reading, Source, Sources},
    store::Store,
};

const RETRY_BASE: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(15 * 60);

/// A process does not run while the computer sleeps, so a jump in the wall
/// clock between two checks means it just woke up.
const WAKE_CHECK: Duration = Duration::from_secs(10);
const WAKE_GAP: Duration = Duration::from_secs(30);

const PRUNE_EVERY: Duration = Duration::from_secs(60 * 60);

/// Told the source ID after every run, whether it succeeded or failed.
pub type OnChange = Arc<dyn Fn(&str) + Send + Sync>;

pub struct Scheduler {
    triggers: HashMap<&'static str, Arc<Notify>>,
}

impl Scheduler {
    pub fn new(sources: &Sources) -> Self {
        Self {
            triggers: sources
                .iter()
                .map(|source| (source.id(), Arc::new(Notify::new())))
                .collect(),
        }
    }

    /// Asks for a run now. Requests made while a run is in progress collapse
    /// into one follow-up run. Returns false for an unknown source.
    pub fn refresh(&self, source_id: &str) -> bool {
        match self.triggers.get(source_id) {
            Some(trigger) => {
                trigger.notify_one();
                true
            }
            None => false,
        }
    }

    fn refresh_all(&self) {
        self.triggers
            .values()
            .for_each(|trigger| trigger.notify_one());
    }

    /// Starts every source, then watches for the computer waking up and keeps
    /// history within the length the user chose.
    pub async fn run(self: Arc<Self>, sources: Sources, store: Arc<Store>, on_change: OnChange) {
        for source in sources {
            let trigger = self.triggers[source.id()].clone();
            tokio::spawn(source_loop(
                source,
                trigger,
                store.clone(),
                on_change.clone(),
            ));
        }

        let mut last_check = SystemTime::now();
        let mut last_prune = None;
        loop {
            if last_prune.is_none_or(|at: Instant| at.elapsed() >= PRUNE_EVERY) {
                if let Err(e) = store.prune_history(now_ms()) {
                    eprintln!("could not prune history: {e}");
                }
                last_prune = Some(Instant::now());
            }

            time::sleep(WAKE_CHECK).await;
            let now = SystemTime::now();
            if now
                .duration_since(last_check)
                .is_ok_and(|gap| gap > WAKE_GAP)
            {
                self.refresh_all();
            }
            last_check = now;
        }
    }
}

async fn source_loop(
    source: Arc<dyn Source>,
    trigger: Arc<Notify>,
    store: Arc<Store>,
    on_change: OnChange,
) {
    let id = source.id();
    let mut failures = 0;

    loop {
        match run_once(&source)
            .await
            .and_then(|readings| save(&store, id, &readings))
        {
            Ok(()) => failures = 0,
            Err(message) => {
                failures += 1;
                if let Err(e) = store.add_error(id, &message, now_ms()) {
                    eprintln!("[{id}] could not record error \"{message}\": {e}");
                }
            }
        }
        on_change(id);

        tokio::select! {
            _ = time::sleep(next_delay(source.interval(), failures)) => {}
            _ = trigger.notified() => {}
        }
    }
}

/// One run in its own task. A time limit stops it, and a panic is contained
/// here as a failed run.
async fn run_once(source: &Arc<dyn Source>) -> Result<Vec<Reading>, String> {
    let keys = secrets::load(source.id(), source.api_keys());
    let running = source.clone();
    let mut task = tokio::spawn(async move { running.run(keys).await });

    match time::timeout(source.timeout(), &mut task).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(format!("{} stopped unexpectedly", source.name())),
        Err(_) => {
            task.abort();
            Err(format!(
                "Timed out after {} seconds",
                source.timeout().as_secs()
            ))
        }
    }
}

fn save(store: &Store, id: &str, readings: &[Reading]) -> Result<(), String> {
    let at = now_ms();
    store
        .save_run(id, readings, at)
        .and_then(|()| store.clear_error(id, at))
        .map_err(|e| format!("Could not save results: {e}"))
}

/// The regular interval, or after a failure, 5s, 10s, 20s and so on up to 15 min.
fn next_delay(interval: Duration, failures: u32) -> Duration {
    match failures {
        0 => interval,
        n => RETRY_BASE
            .saturating_mul(2u32.saturating_pow(n - 1))
            .min(RETRY_MAX),
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}
