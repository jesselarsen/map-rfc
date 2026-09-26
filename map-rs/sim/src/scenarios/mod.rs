//! Scenario registry (one module per pre-registered experiment).

use crate::runner::Scenario;

pub mod e1;
pub mod e2;
pub mod e3;
pub mod e4;
pub mod kad;
pub mod f1;

pub const ALL: &[Scenario] = &[
    Scenario {
        name: "f1",
        metrics: f1::METRICS,
        run: f1::run,
    },
    Scenario {
        name: "e1",
        metrics: e1::METRICS,
        run: e1::run,
    },
    Scenario {
        name: "e2",
        metrics: e2::METRICS,
        run: e2::run,
    },
    Scenario {
        name: "e3a",
        metrics: e3::METRICS_A,
        run: e3::run_a,
    },
    Scenario {
        name: "e3b",
        metrics: e3::METRICS_B,
        run: e3::run_b,
    },
    Scenario {
        name: "e3c",
        metrics: kad::METRICS,
        run: kad::run,
    },
    Scenario {
        name: "e3d",
        metrics: e3::METRICS_D,
        run: e3::run_d,
    },
    Scenario {
        name: "e4",
        metrics: e4::METRICS,
        run: e4::run,
    },
];

pub fn find(name: &str) -> Option<&'static Scenario> {
    ALL.iter().find(|s| s.name == name)
}
