use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use serde_json::json;
use sysinfo::{Disks, MINIMUM_CPU_UPDATE_INTERVAL, Networks, System};

use super::{Metric, Reading, Source, Unit, Widget, WidgetType};

const INTERVAL: Duration = Duration::from_secs(5);

/// CPU load and network speed are differences between two samples, so unlike
/// other sources this one has to keep its OS counters between runs. It holds
/// no results: the first run after a start takes its own baseline.
#[derive(Default)]
pub struct SystemResources {
    /// Created by the first run, so that a failure is reported like any other.
    sampler: Arc<Mutex<Option<Sampler>>>,
}

#[async_trait]
impl Source for SystemResources {
    fn id(&self) -> &'static str {
        "system"
    }

    fn name(&self) -> &'static str {
        "System Resources"
    }

    fn interval(&self) -> Duration {
        INTERVAL
    }

    fn widgets(&self) -> Vec<Widget> {
        vec![
            Widget { id: "cpu", kind: WidgetType::Chart, title: "CPU", keys: &["cpu.usage"] },
            Widget {
                id: "memory",
                kind: WidgetType::Meter,
                title: "Memory",
                keys: &["memory.usage", "memory.used", "memory.total"],
            },
            Widget { id: "uptime", kind: WidgetType::Stat, title: "Uptime", keys: &["uptime"] },
            Widget {
                id: "network",
                kind: WidgetType::Chart,
                title: "Network",
                keys: &["net.down", "net.up"],
            },
            Widget { id: "cores", kind: WidgetType::Bars, title: "CPU cores", keys: &["cpu.cores"] },
            Widget { id: "disks", kind: WidgetType::List, title: "Disks", keys: &["disks"] },
        ]
    }

    fn metrics(&self) -> Vec<Metric> {
        vec![
            Metric { key: "cpu.usage", label: "CPU", unit: Unit::Percent },
            Metric { key: "memory.usage", label: "Memory", unit: Unit::Percent },
            Metric { key: "memory.used", label: "Memory used", unit: Unit::Bytes },
            Metric { key: "memory.total", label: "Memory total", unit: Unit::Bytes },
            Metric { key: "net.down", label: "Download", unit: Unit::BytesPerSecond },
            Metric { key: "net.up", label: "Upload", unit: Unit::BytesPerSecond },
            Metric { key: "uptime", label: "Uptime", unit: Unit::Seconds },
        ]
    }

    async fn run(&self, _api_keys: HashMap<String, String>) -> Result<Vec<Reading>, String> {
        let sampler = self.sampler.clone();
        tokio::task::spawn_blocking(move || {
            // A panic mid-sample leaves only counters behind, which are safe to reuse.
            let mut sampler = sampler.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            match sampler.as_mut() {
                Some(sampler) => sampler.sample(),
                None => sampler.insert(Sampler::new().map_err(describe)?).sample(),
            }
        })
        .await
        .map_err(|e| format!("System sampling stopped: {e}"))?
    }
}

struct Sampler {
    system: System,
    networks: Networks,
    disks: Disks,
    last_sample: Option<Instant>,
}

impl Sampler {
    fn new() -> Result<Self, sysinfo::Error> {
        Ok(Self {
            system: System::new()?,
            networks: Networks::new_with_refreshed_list()?,
            disks: Disks::new_with_refreshed_list()?,
            last_sample: None,
        })
    }

    fn refresh(&mut self) {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.networks.refresh(true);
        self.disks.refresh(true);
    }

    fn sample(&mut self) -> Result<Vec<Reading>, String> {
        let last_sample = match self.last_sample {
            Some(at) => at,
            None => {
                let at = Instant::now();
                self.refresh();
                thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
                at
            }
        };

        self.refresh();
        let now = Instant::now();
        let elapsed = now.duration_since(last_sample).as_secs_f64().max(0.001);
        self.last_sample = Some(now);

        let cpus = self.system.cpus();
        if cpus.is_empty() {
            return Err("Windows reported no CPU data".to_owned());
        }

        let cores: Vec<f64> = cpus.iter().map(|cpu| round(cpu.usage() as f64)).collect();
        let used = self.system.used_memory();
        let total = self.system.total_memory();
        let (down, up) = self
            .networks
            .iter()
            .filter(|(name, _)| !name.to_string_lossy().to_lowercase().contains("loopback"))
            .fold((0, 0), |(down, up), (_, data)| {
                (down + data.received(), up + data.transmitted())
            });
        let disks: Vec<_> = self
            .disks
            .iter()
            .filter(|disk| disk.total_space() > 0)
            .map(|disk| {
                let mount = disk.mount_point().to_string_lossy().into_owned();
                let name = disk.name().to_string_lossy().into_owned();
                json!({
                    "name": if name.is_empty() { "Local Disk".to_owned() } else { name },
                    "detail": mount.trim_end_matches('\\'),
                    "used": disk.total_space().saturating_sub(disk.available_space()),
                    "total": disk.total_space(),
                })
            })
            .collect();

        Ok(vec![
            Reading::new("cpu.usage", round(self.system.global_cpu_usage() as f64)),
            Reading::new("cpu.cores", cores),
            Reading::new("memory.usage", round(used as f64 / total.max(1) as f64 * 100.0)),
            Reading::new("memory.used", used),
            Reading::new("memory.total", total),
            Reading::new("net.down", (down as f64 / elapsed).round()),
            Reading::new("net.up", (up as f64 / elapsed).round()),
            Reading::new("disks", disks),
            Reading::new("uptime", System::uptime().map_err(describe)?),
        ])
    }
}

fn describe(error: sysinfo::Error) -> String {
    format!("Could not read system data: {error}")
}

fn round(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}
