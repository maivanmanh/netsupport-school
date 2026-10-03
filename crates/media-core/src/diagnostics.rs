use std::{
    collections::{BTreeMap, VecDeque},
    time::Duration,
};

/// Bounded samples plus exact count/sum/max, drained each reporting interval.
#[derive(Default)]
pub struct StageMetrics(BTreeMap<&'static str, Samples>);
#[derive(Default)]
struct Samples {
    recent: VecDeque<Duration>,
    count: u64,
    total_us: u128,
    max: Duration,
}
#[derive(Debug)]
pub struct StageSummary {
    pub stage: &'static str,
    pub samples: u64,
    pub retained_samples: usize,
    pub mean_us: u64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub max_us: u64,
}
impl StageMetrics {
    pub fn record(&mut self, stage: &'static str, duration: Duration) {
        let sample = self.0.entry(stage).or_default();
        if sample.recent.len() == 1024 {
            sample.recent.pop_front();
        }
        sample.recent.push_back(duration);
        sample.count += 1;
        sample.total_us += duration.as_micros();
        sample.max = sample.max.max(duration);
    }
    #[must_use]
    pub fn summaries(self) -> Vec<StageSummary> {
        self.0
            .into_iter()
            .map(|(stage, sample)| {
                let mut sorted: Vec<_> = sample.recent.into_iter().collect();
                sorted.sort_unstable();
                let at = |percent: usize| {
                    u64::try_from(sorted[(sorted.len() - 1) * percent / 100].as_micros())
                        .unwrap_or(u64::MAX)
                };
                StageSummary {
                    stage,
                    samples: sample.count,
                    retained_samples: sorted.len(),
                    mean_us: u64::try_from(sample.total_us / u128::from(sample.count))
                        .unwrap_or(u64::MAX),
                    p50_us: at(50),
                    p95_us: at(95),
                    p99_us: at(99),
                    max_us: u64::try_from(sample.max.as_micros()).unwrap_or(u64::MAX),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_exact_totals_while_bounding_percentile_storage() {
        let mut metrics = StageMetrics::default();
        for us in 1..=2048 {
            metrics.record("stage", Duration::from_micros(us));
        }
        let summary = metrics.summaries().pop().unwrap();
        assert_eq!(summary.samples, 2048);
        assert_eq!(summary.mean_us, 1024);
        assert_eq!(summary.max_us, 2048);
        assert_eq!(summary.p50_us, 1536);
        assert!(StageMetrics::default().summaries().is_empty());
    }
}
