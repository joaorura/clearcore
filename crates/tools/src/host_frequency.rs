use std::{collections::BTreeMap, fmt, marker::PhantomData};

use serde::{Deserialize, Deserializer, Serialize, de::SeqAccess, de::Visitor};

use crate::host_observation::{CpuTopologyEntry, LocalHostFacts};

const OBSERVATION_MILLISECONDS: u64 = 300_000;
const MAXIMUM_SAMPLE_GAP_MILLISECONDS: u64 = 1_000;
const REQUIRED_SNAPSHOTS: usize = 301;
const MAXIMUM_LOGICAL_CPUS: usize = 256;
const MINIMUM_FREQUENCY_MHZ: f64 = 2_000.0;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SustainedFrequencyObservation {
    snapshots: BoundedVec<FrequencySnapshot, REQUIRED_SNAPSHOTS>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FrequencySnapshot {
    elapsed_milliseconds: u64,
    cores: BoundedVec<CoreFrequencySample, MAXIMUM_LOGICAL_CPUS>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CoreFrequencySample {
    logical_cpu: usize,
    physical_package_id: usize,
    core_id: usize,
    frequency_mhz: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SustainedFrequencySummary {
    pub minimum_mhz: f64,
    pub snapshot_count: usize,
    pub maximum_gap_milliseconds: u64,
    pub sampled_logical_cpus: Vec<usize>,
}

pub fn validate_sustained_frequency(
    observation: &SustainedFrequencyObservation,
    local: &LocalHostFacts,
) -> Result<SustainedFrequencySummary, &'static str> {
    let required_topology = required_topology(local)?;
    let snapshots = observation.snapshots.as_slice();
    let first = snapshots.first().ok_or("frequency observation is empty")?;
    let last = snapshots.last().ok_or("frequency observation is empty")?;
    if snapshots.len() != REQUIRED_SNAPSHOTS
        || first.elapsed_milliseconds != 0
        || last.elapsed_milliseconds != OBSERVATION_MILLISECONDS
    {
        return Err("frequency observation does not cover the full window");
    }

    let mut minimum_mhz = f64::INFINITY;
    let mut maximum_gap_milliseconds = 0;
    for (index, snapshot) in snapshots.iter().enumerate() {
        if let Some(previous) = index.checked_sub(1).and_then(|prior| snapshots.get(prior)) {
            let gap = snapshot
                .elapsed_milliseconds
                .checked_sub(previous.elapsed_milliseconds)
                .ok_or("frequency timestamps are not monotonic")?;
            if gap == 0 || gap > MAXIMUM_SAMPLE_GAP_MILLISECONDS {
                return Err("frequency sample cadence is incomplete");
            }
            maximum_gap_milliseconds = maximum_gap_milliseconds.max(gap);
        }
        validate_snapshot(snapshot, &required_topology, &mut minimum_mhz)?;
    }

    Ok(SustainedFrequencySummary {
        minimum_mhz,
        snapshot_count: snapshots.len(),
        maximum_gap_milliseconds,
        sampled_logical_cpus: required_topology.keys().copied().collect(),
    })
}

fn required_topology(
    local: &LocalHostFacts,
) -> Result<BTreeMap<usize, &CpuTopologyEntry>, &'static str> {
    if local.affinity_cpus.is_empty() || local.topology.is_empty() {
        return Err("local CPU topology is unavailable");
    }
    let topology = local
        .topology
        .iter()
        .map(|entry| (entry.logical_cpu, entry))
        .collect::<BTreeMap<_, _>>();
    let topology_cpus = topology.keys().copied().collect::<Vec<_>>();
    if topology.len() != local.topology.len() || local.affinity_cpus != topology_cpus {
        return Err("process affinity does not cover the complete CPU topology");
    }
    Ok(topology)
}

fn validate_snapshot(
    snapshot: &FrequencySnapshot,
    required_topology: &BTreeMap<usize, &CpuTopologyEntry>,
    minimum_mhz: &mut f64,
) -> Result<(), &'static str> {
    let cores = snapshot.cores.as_slice();
    if cores.len() != required_topology.len() {
        return Err("frequency snapshot does not cover required logical CPUs");
    }
    for (sample, (logical_cpu, topology)) in cores.iter().zip(required_topology) {
        if sample.logical_cpu != *logical_cpu
            || sample.physical_package_id != topology.physical_package_id
            || sample.core_id != topology.core_id
        {
            return Err("frequency sample CPU topology mismatch");
        }
        if !sample.frequency_mhz.is_finite() || sample.frequency_mhz < MINIMUM_FREQUENCY_MHZ {
            return Err("frequency sample is below the sustained threshold");
        }
        *minimum_mhz = minimum_mhz.min(sample.frequency_mhz);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct BoundedVec<T, const MAXIMUM: usize>(Vec<T>);

impl<T, const MAXIMUM: usize> BoundedVec<T, MAXIMUM> {
    fn as_slice(&self) -> &[T] {
        &self.0
    }
}

impl<'de, T, const MAXIMUM: usize> Deserialize<'de> for BoundedVec<T, MAXIMUM>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(BoundedVecVisitor::<T, MAXIMUM>(PhantomData))
    }
}

struct BoundedVecVisitor<T, const MAXIMUM: usize>(PhantomData<T>);

impl<'de, T, const MAXIMUM: usize> Visitor<'de> for BoundedVecVisitor<T, MAXIMUM>
where
    T: Deserialize<'de>,
{
    type Value = BoundedVec<T, MAXIMUM>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "a sequence with at most {MAXIMUM} entries")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAXIMUM));
        while let Some(value) = sequence.next_element()? {
            if values.len() == MAXIMUM {
                return Err(serde::de::Error::custom("bounded sequence exceeds limit"));
            }
            values.push(value);
        }
        Ok(BoundedVec(values))
    }
}
