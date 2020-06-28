use crate::event::metric::StatisticKind;
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub min: f64,
    pub max: f64,
    pub median: f64,
    pub avg: f64,
    pub sum: f64,
    pub count: f64,
    pub quantiles: Vec<(f64, f64)>,
}

impl Summary {
    pub fn new(values: &[f64], counts: &[u32], kind: StatisticKind) -> Option<Self> {
        if values.len() != counts.len() {
            return None;
        }

        let mut samples = Vec::new();
        for (v, c) in values.iter().zip(counts.iter()) {
            for _ in 0..*c {
                samples.push(*v);
            }
        }

        if samples.is_empty() {
            return None;
        }

        if samples.len() == 1 {
            let val = samples[0];
            return Some(Summary {
                min: val,
                max: val,
                median: val,
                avg: val,
                sum: val,
                count: 1.0,
                quantiles: vec![(0.95, val)],
            });
        }

        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

        let length = samples.len() as f64;
        let min = samples.first().unwrap();
        let max = samples.last().unwrap();

        let p50 = samples[(0.50 * length - 1.0).round() as usize];
        let quantiles = kind
            .percentile()
            .iter()
            .map(|&p| (p, samples[(p * length - 1.0).round() as usize]))
            .collect();

        let sum = samples.iter().sum();
        let avg = sum / length;

        Some(Summary {
            min: *min,
            max: *max,
            median: p50,
            avg,
            sum,
            count: length,
            quantiles,
        })
    }
}

impl StatisticKind {
    pub fn percentile(self) -> &'static [f64] {
        match self {
            Self::Histogram => &[0.95],
            Self::Distribution => &[0.5, 0.75, 0.9, 0.95, 0.99],
        }
    }
}
