use std::{collections::BTreeSet, env, fs, path::Path, process::Command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHostFacts {
    pub os: String,
    pub architecture: String,
    pub cpu_model: String,
    pub physical_cores: Option<usize>,
    pub avx2: bool,
    pub ram_kib: Option<u64>,
    pub virtualized: Option<bool>,
    pub ac_power: Option<bool>,
    pub affinity_cpus: Vec<usize>,
    pub governor: Option<String>,
}

pub fn observe_local_host() -> LocalHostFacts {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let affinity_cpus = parse_cpu_list(read_field("/proc/self/status", "Cpus_allowed_list:"));
    LocalHostFacts {
        os: env::consts::OS.to_owned(),
        architecture: env::consts::ARCH.to_owned(),
        cpu_model: cpuinfo
            .lines()
            .find_map(|line| line.strip_prefix("model name\t: "))
            .unwrap_or("unavailable")
            .to_owned(),
        physical_cores: physical_core_count(&cpuinfo),
        avx2: cpuinfo.lines().any(|line| {
            line.starts_with("flags") && line.split_whitespace().any(|flag| flag == "avx2")
        }),
        ram_kib: read_field("/proc/meminfo", "MemTotal:").and_then(|value| {
            value
                .split_whitespace()
                .next()
                .and_then(|raw| raw.parse().ok())
        }),
        virtualized: virtualization_observed(&cpuinfo),
        ac_power: observe_ac_power(),
        governor: observe_governor(&affinity_cpus),
        affinity_cpus,
    }
}

fn physical_core_count(cpuinfo: &str) -> Option<usize> {
    let cores = cpuinfo
        .split("\n\n")
        .filter_map(|processor| {
            let physical = processor
                .lines()
                .find_map(|line| line.strip_prefix("physical id\t: "))?;
            let core = processor
                .lines()
                .find_map(|line| line.strip_prefix("core id\t\t: "))?;
            Some((physical.to_owned(), core.to_owned()))
        })
        .collect::<BTreeSet<_>>()
        .len();
    (cores > 0).then_some(cores)
}

fn virtualization_observed(cpuinfo: &str) -> Option<bool> {
    let marker_detected = Path::new("/.dockerenv").exists()
        || cpuinfo.lines().any(|line| {
            line.starts_with("flags") && line.split_whitespace().any(|flag| flag == "hypervisor")
        })
        || fs::read_to_string("/proc/1/cgroup").is_ok_and(|contents| {
            ["docker", "containerd", "kubepods", "lxc"]
                .iter()
                .any(|marker| contents.contains(marker))
        });
    if marker_detected {
        return Some(true);
    }
    Command::new("systemd-detect-virt")
        .arg("--quiet")
        .status()
        .ok()
        .and_then(|status| match status.code() {
            Some(0) => Some(true),
            Some(1) => Some(false),
            _ => None,
        })
}

fn observe_ac_power() -> Option<bool> {
    let mut supplies = fs::read_dir("/sys/class/power_supply")
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    supplies.sort();
    supplies.into_iter().find_map(|path| {
        (fs::read_to_string(path.join("type")).ok()?.trim() == "Mains").then(|| {
            fs::read_to_string(path.join("online"))
                .ok()
                .map(|value| value.trim() == "1")
        })?
    })
}

fn observe_governor(cpus: &[usize]) -> Option<String> {
    let governors = cpus
        .iter()
        .map(|cpu| {
            fs::read_to_string(format!(
                "/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_governor"
            ))
            .map(|value| value.trim().to_owned())
        })
        .collect::<Result<BTreeSet<_>, _>>()
        .ok()?;
    (governors.len() == 1)
        .then(|| governors.into_iter().next())
        .flatten()
}

fn read_field(path: &str, prefix: &str) -> Option<String> {
    fs::read_to_string(path).ok()?.lines().find_map(|line| {
        line.strip_prefix(prefix)
            .map(|value| value.trim().to_owned())
    })
}

fn parse_cpu_list(value: Option<String>) -> Vec<usize> {
    let mut cpus = value
        .into_iter()
        .flat_map(|list| list.split(',').map(str::to_owned).collect::<Vec<_>>())
        .flat_map(|part| match part.split_once('-') {
            Some((start, end)) => match (start.parse::<usize>(), end.parse::<usize>()) {
                (Ok(start), Ok(end)) => (start..=end).collect(),
                _ => Vec::new(),
            },
            None => part.parse().map_or_else(|_| Vec::new(), |cpu| vec![cpu]),
        })
        .collect::<Vec<_>>();
    cpus.sort_unstable();
    cpus.dedup();
    cpus
}
