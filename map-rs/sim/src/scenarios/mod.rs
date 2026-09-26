//! Scenario registry (one module per pre-registered experiment).

use crate::runner::Scenario;

pub mod e2;
pub mod f1;

pub const ALL: &[Scenario] = &[
    Scenario {
        name: "f1",
        metrics: f1::METRICS,
        run: f1::run,
    },
    Scenario {
        name: "e2",
        metrics: e2::METRICS,
        run: e2::run,
    },
];

pub fn find(name: &str) -> Option<&'static Scenario> {
    ALL.iter().find(|s| s.name == name)
}
