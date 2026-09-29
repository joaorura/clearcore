use std::{collections::BTreeSet, env, fs, path::Path, process::Command};

use serde::Serialize;

const MAXIMUM_LOGICAL_CPUS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CpuTopologyEntry {
    pub logical_cpu: usize,
    pub physical_package_id: usize,
    pub core_id: usize,
}

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
    pub topology: Vec<CpuTopologyEntry>,
}

pub fn observe_local_host() -> LocalHostFacts {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let affinity_cpus = parse_cpu_list(read_field("/proc/self/status", "Cpus_allowed_list:"));
    let topology = cpu_topology(&cpuinfo);
    LocalHostFacts {
        os: env::consts::OS.to_owned(),
        architecture: env::consts::ARCH.to_owned(),
        cpu_model: cpuinfo
            .lines()
            .find_map(|line| line.strip_prefix("model name\t: "))
            .unwrap_or("unavailable")
            .to_owned(),
        physical_cores: physical_core_count(&topology),
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
        topology,
    }
}

fn cpu_topology(cpuinfo: &str) -> Vec<CpuTopologyEntry> {
    let processor_blocks = cpuinfo
        .split("\n\n")
        .filter(|processor| processor.lines().any(|line| line.starts_with("processor")))
        .collect::<Vec<_>>();
    let Some(mut topology) = processor_blocks
        .iter()
        .map(|processor| {
            let logical_cpu = processor
                .lines()
                .find_map(|line| line.strip_prefix("processor\t: "))?
                .parse()
                .ok()?;
            let physical = processor
                .lines()
                .find_map(|line| line.strip_prefix("physical id\t: "))?
                .parse()
                .ok()?;
            let core = processor
                .lines()
                .find_map(|line| line.strip_prefix("core id\t\t: "))?
                .parse()
                .ok()?;
            Some(CpuTopologyEntry {
                logical_cpu,
                physical_package_id: physical,
                core_id: core,
            })
        })
        .collect::<Option<Vec<_>>>()
    else {
        return Vec::new();
    };
    topology.sort_by_key(|entry| entry.logical_cpu);
    if topology.is_empty()
        || topology.len() > MAXIMUM_LOGICAL_CPUS
        || topology
            .windows(2)
            .any(|entries| entries[0].logical_cpu == entries[1].logical_cpu)
    {
        return Vec::new();
    }
    topology
}

fn physical_core_count(topology: &[CpuTopologyEntry]) -> Option<usize> {
    let cores = topology
        .iter()
        .map(|entry| (entry.physical_package_id, entry.core_id))
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
    let Some(value) = value else {
        return Vec::new();
    };
    let mut cpus = BTreeSet::new();
    for part in value.split(',') {
        let (start, end) = match part.split_once('-') {
            Some((start, end)) => match (start.parse::<usize>(), end.parse::<usize>()) {
                (Ok(start), Ok(end)) => (start, end),
                _ => return Vec::new(),
            },
            None => match part.parse::<usize>() {
                Ok(cpu) => (cpu, cpu),
                Err(_) => return Vec::new(),
            },
        };
        let Some(range_len) = end.checked_sub(start).and_then(|span| span.checked_add(1)) else {
            return Vec::new();
        };
        if range_len > MAXIMUM_LOGICAL_CPUS || cpus.len() + range_len > MAXIMUM_LOGICAL_CPUS {
            return Vec::new();
        }
        cpus.extend(start..=end);
    }
    cpus.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::{cpu_topology, parse_cpu_list};

    #[test]
    fn incomplete_cpu_topology_fails_closed() {
        let cpuinfo = "processor\t: 0\nphysical id\t: 0\ncore id\t\t: 0\n\nprocessor\t: 1\nphysical id\t: 0\n";

        assert!(cpu_topology(cpuinfo).is_empty());
    }

    #[test]
    fn duplicate_logical_cpu_topology_fails_closed() {
        let cpuinfo = "processor\t: 0\nphysical id\t: 0\ncore id\t\t: 0\n\nprocessor\t: 0\nphysical id\t: 0\ncore id\t\t: 1\n";

        assert!(cpu_topology(cpuinfo).is_empty());
    }

    #[test]
    fn oversized_cpu_affinity_range_fails_closed() {
        assert!(parse_cpu_list(Some("0-1000000".to_owned())).is_empty());
    }
}
