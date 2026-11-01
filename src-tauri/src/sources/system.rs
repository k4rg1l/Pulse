use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use nvml_wrapper::{Nvml, enum_wrappers::device::TemperatureSensor, error::NvmlError};
use serde_json::json;
use sysinfo::{Disks, MINIMUM_CPU_UPDATE_INTERVAL, Networks, System};

use super::{Metric, Reading, Source, Unit, Widget, WidgetType};

const INTERVAL: Duration = Duration::from_secs(5);

const GPU_KEY_PREFIX: &str = "gpu.";

/// CPU load and network speed are differences between two samples, so unlike
/// other sources this one has to keep its OS counters between runs. It holds
/// no results: the first run after a start takes its own baseline.
#[derive(Default)]
pub struct SystemResources {
    /// Created by the first run, so that a failure is reported like any other.
    sampler: Arc<Mutex<Option<Sampler>>>,
    /// Checked once. Graphics hardware does not change while Pulse runs.
    has_supported_gpu: OnceLock<bool>,
}

impl SystemResources {
    /// GPU figures come from NVML, which only NVIDIA hardware provides. Without it
    /// the GPU widgets and metrics are left out, rather than sitting empty.
    fn has_supported_gpu(&self) -> bool {
        *self.has_supported_gpu.get_or_init(|| {
            Nvml::init().is_ok_and(|nvml| nvml.device_count().is_ok_and(|count| count > 0))
        })
    }
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
        let mut widgets = vec![
            Widget {
                id: "cpu",
                kind: WidgetType::Chart,
                title: "CPU",
                keys: &["cpu.usage"],
                note: Some("system.processor"),
            },
            Widget {
                id: "cores",
                kind: WidgetType::Bars,
                title: "CPU cores",
                keys: &["cpu.cores"],
                note: Some("system.cores"),
            },
            Widget {
                id: "gpu",
                kind: WidgetType::Chart,
                title: "GPU",
                keys: &["gpu.usage"],
                note: Some("system.graphics"),
            },
            Widget {
                id: "vram",
                kind: WidgetType::Meter,
                title: "GPU memory",
                keys: &["gpu.memory.usage", "gpu.memory.used", "gpu.memory.total"],
                note: None,
            },
            Widget {
                id: "gpu-temperature",
                kind: WidgetType::Stat,
                title: "GPU temp",
                keys: &["gpu.temperature"],
                note: None,
            },
            Widget {
                id: "memory",
                kind: WidgetType::Meter,
                title: "Memory",
                keys: &["memory.usage", "memory.used", "memory.total"],
                note: None,
            },
            Widget {
                id: "uptime",
                kind: WidgetType::Stat,
                title: "Uptime",
                keys: &["uptime"],
                note: None,
            },
            Widget {
                id: "disks",
                kind: WidgetType::List,
                title: "Disks",
                keys: &["disks"],
                note: None,
            },
            Widget {
                id: "network",
                kind: WidgetType::MirrorChart,
                title: "Network",
                keys: &["net.down", "net.up"],
                note: None,
            },
            Widget {
                id: "machine",
                kind: WidgetType::Colophon,
                title: "This computer",
                keys: &["system.machine"],
                note: None,
            },
        ];
        if !self.has_supported_gpu() {
            widgets.retain(|widget| !widget.keys[0].starts_with(GPU_KEY_PREFIX));
        }
        widgets
    }

    fn metrics(&self) -> Vec<Metric> {
        let mut metrics = vec![
            Metric {
                key: "cpu.usage",
                label: "CPU",
                unit: Unit::Percent,
            },
            Metric {
                key: "memory.usage",
                label: "Memory",
                unit: Unit::Percent,
            },
            Metric {
                key: "memory.used",
                label: "Memory used",
                unit: Unit::Bytes,
            },
            Metric {
                key: "memory.total",
                label: "Memory total",
                unit: Unit::Bytes,
            },
            Metric {
                key: "gpu.usage",
                label: "GPU",
                unit: Unit::Percent,
            },
            Metric {
                key: "gpu.memory.usage",
                label: "GPU memory",
                unit: Unit::Percent,
            },
            Metric {
                key: "gpu.memory.used",
                label: "GPU memory used",
                unit: Unit::Bytes,
            },
            Metric {
                key: "gpu.temperature",
                label: "GPU temperature",
                unit: Unit::Celsius,
            },
            Metric {
                key: "net.down",
                label: "Download",
                unit: Unit::BytesPerSecond,
            },
            Metric {
                key: "net.up",
                label: "Upload",
                unit: Unit::BytesPerSecond,
            },
            Metric {
                key: "uptime",
                label: "Uptime",
                unit: Unit::Seconds,
            },
        ];
        if !self.has_supported_gpu() {
            metrics.retain(|metric| !metric.key.starts_with(GPU_KEY_PREFIX));
        }
        metrics
    }

    async fn run(&self, _api_keys: HashMap<String, String>) -> Result<Vec<Reading>, String> {
        let sampler = self.sampler.clone();
        tokio::task::spawn_blocking(move || {
            // A panic mid-sample leaves only counters behind, which are safe to reuse.
            let mut sampler = sampler
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
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
    /// Absent without an NVIDIA driver. The first GPU is the one reported.
    nvml: Option<Nvml>,
    /// Fixed for the life of the app, so read once.
    identity: Option<Identity>,
}

impl Sampler {
    fn new() -> Result<Self, sysinfo::Error> {
        Ok(Self {
            system: System::new()?,
            networks: Networks::new_with_refreshed_list()?,
            disks: Disks::new_with_refreshed_list()?,
            last_sample: None,
            nvml: Nvml::init().ok(),
            identity: None,
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

        let identity = self
            .identity
            .get_or_insert_with(|| read_identity(&self.system, self.nvml.as_ref()));

        let mut readings = vec![
            Reading::current_only("system.processor", identity.processor.as_str()),
            Reading::current_only("system.graphics", identity.graphics.as_str()),
            Reading::current_only("system.cores", identity.cores.as_str()),
            Reading::current_only("system.machine", identity.machine.as_str()),
            Reading::new("cpu.usage", round(self.system.global_cpu_usage() as f64)),
            Reading::current_only("cpu.cores", cores),
            Reading::current_only(
                "memory.usage",
                round(used as f64 / total.max(1) as f64 * 100.0),
            ),
            Reading::current_only("memory.used", used),
            Reading::current_only("memory.total", total),
            Reading::new("net.down", (down as f64 / elapsed).round()),
            Reading::new("net.up", (up as f64 / elapsed).round()),
            Reading::current_only("disks", disks),
            Reading::current_only("uptime", System::uptime().map_err(describe)?),
        ];

        // A GPU problem leaves its widgets on their last values. The rest still updates.
        if let Some(gpu) = self.nvml.as_ref().and_then(|nvml| read_gpu(nvml).ok()) {
            readings.extend(gpu);
        }

        Ok(readings)
    }
}

fn read_gpu(nvml: &Nvml) -> Result<Vec<Reading>, NvmlError> {
    let device = nvml.device_by_index(0)?;
    let usage = device.utilization_rates()?;
    let memory = device.memory_info()?;
    let temperature = device.temperature(TemperatureSensor::Gpu)?;

    Ok(vec![
        Reading::new("gpu.usage", usage.gpu),
        Reading::current_only(
            "gpu.memory.usage",
            round(memory.used as f64 / memory.total.max(1) as f64 * 100.0),
        ),
        Reading::current_only("gpu.memory.used", memory.used),
        Reading::current_only("gpu.memory.total", memory.total),
        Reading::current_only("gpu.temperature", temperature),
    ])
}

/// The names printed on each instrument.
struct Identity {
    processor: String,
    graphics: String,
    cores: String,
    machine: String,
}

fn read_identity(system: &System, nvml: Option<&Nvml>) -> Identity {
    let threads = system.cpus().len();
    let cores = System::physical_core_count().map_or_else(
        |_| format!("{threads} threads"),
        |cores| format!("{cores} cores, {threads} threads"),
    );
    let graphics = nvml
        .and_then(|nvml| nvml.device_by_index(0).ok()?.name().ok())
        .unwrap_or_default();

    Identity {
        processor: system
            .cpus()
            .first()
            .map_or_else(String::new, |cpu| short_processor(cpu.brand())),
        graphics: short_graphics(&graphics).to_owned(),
        cores,
        machine: format!(
            "{} on {}",
            System::host_name().unwrap_or_default(),
            System::long_os_version().unwrap_or_default()
        ),
    }
}

/// The model alone: "13th Gen Intel Core i7-13700K" becomes "i7-13700K".
fn short_processor(brand: &str) -> String {
    let brand = brand.replace("(R)", "").replace("(TM)", "");
    let model = brand
        .split_once(" Core ")
        .map_or(brand.as_str(), |(_, model)| model);
    model
        .trim_start_matches("AMD ")
        .split_whitespace()
        .take_while(|word| !word.ends_with("-Core"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The model alone: "NVIDIA GeForce RTX 4070 Ti" becomes "RTX 4070 Ti".
fn short_graphics(name: &str) -> &str {
    name.trim_start_matches("NVIDIA ")
        .trim_start_matches("GeForce ")
}

fn describe(error: sysinfo::Error) -> String {
    format!("Could not read system data: {error}")
}

fn round(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}
