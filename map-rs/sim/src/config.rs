//! Experiment configs (TOML). A config names a scenario, a seed count,
//! fixed `[params]` and a `[sweep]` whose keys are expanded as a cartesian
//! product (each key becomes a CSV column):
//!
//! ```toml
//! scenario = "f1"
//! seeds = 60
//! [params]
//! n = 100
//! [sweep]
//! alpha = [0.0, 0.1]
//! policy = ["noverify", "riskaware"]
//! ```

use serde::Deserialize;
use std::collections::BTreeMap;
use toml::Value;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub scenario: String,
    /// Output file stem (defaults to the scenario name).
    #[serde(default)]
    pub name: Option<String>,
    pub seeds: u64,
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
    #[serde(default)]
    pub sweep: BTreeMap<String, Vec<Value>>,
}

#[derive(Deserialize)]
struct Many {
    experiment: Vec<Config>,
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// A file holds either one experiment or `[[experiment]]` tables.
    pub fn parse_many(text: &str) -> Result<Vec<Config>, String> {
        if let Ok(m) = toml::from_str::<Many>(text) {
            return Ok(m.experiment);
        }
        Ok(vec![Config::parse(text)?])
    }

    pub fn stem(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.scenario)
    }

    /// Every sweep point, in a fixed order: keys sorted, the last key
    /// varying fastest.
    pub fn points(&self) -> Vec<Params> {
        let keys: Vec<&String> = self.sweep.keys().collect();
        let mut out = vec![BTreeMap::new()];
        for k in &keys {
            let mut next = Vec::new();
            for base in &out {
                for v in &self.sweep[*k] {
                    let mut m: BTreeMap<String, Value> = base.clone();
                    m.insert((*k).clone(), v.clone());
                    next.push(m);
                }
            }
            out = next;
        }
        out.into_iter()
            .map(|swept| {
                let mut all = self.params.clone();
                all.extend(swept.clone());
                Params { all, swept }
            })
            .collect()
    }
}

/// One sweep point: fixed params overlaid with the swept values.
#[derive(Clone, Debug)]
pub struct Params {
    all: BTreeMap<String, Value>,
    /// Only the swept keys (the CSV point columns).
    pub swept: BTreeMap<String, Value>,
}

impl Params {
    pub fn from_pairs(pairs: &[(&str, Value)]) -> Params {
        let all: BTreeMap<String, Value> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        Params {
            swept: all.clone(),
            all,
        }
    }

    fn get(&self, k: &str) -> Result<&Value, String> {
        self.all
            .get(k)
            .ok_or_else(|| format!("missing parameter `{k}`"))
    }

    pub fn f64(&self, k: &str) -> Result<f64, String> {
        match self.get(k)? {
            Value::Float(f) => Ok(*f),
            Value::Integer(i) => Ok(*i as f64),
            v => Err(format!("`{k}` must be a number, got {v}")),
        }
    }

    pub fn f64_or(&self, k: &str, d: f64) -> Result<f64, String> {
        if self.all.contains_key(k) {
            self.f64(k)
        } else {
            Ok(d)
        }
    }

    pub fn u64(&self, k: &str) -> Result<u64, String> {
        match self.get(k)? {
            Value::Integer(i) if *i >= 0 => Ok(*i as u64),
            v => Err(format!("`{k}` must be a non-negative integer, got {v}")),
        }
    }

    pub fn u64_or(&self, k: &str, d: u64) -> Result<u64, String> {
        if self.all.contains_key(k) {
            self.u64(k)
        } else {
            Ok(d)
        }
    }

    pub fn str(&self, k: &str) -> Result<&str, String> {
        match self.get(k)? {
            Value::String(s) => Ok(s),
            v => Err(format!("`{k}` must be a string, got {v}")),
        }
    }

    pub fn bool_or(&self, k: &str, d: bool) -> Result<bool, String> {
        match self.all.get(k) {
            None => Ok(d),
            Some(Value::Boolean(b)) => Ok(*b),
            Some(v) => Err(format!("`{k}` must be a boolean, got {v}")),
        }
    }
}

/// Render a swept value for a CSV cell.
pub fn cell(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Float(f) => format!("{f}"),
        Value::Integer(i) => format!("{i}"),
        Value::Boolean(b) => format!("{b}"),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_is_a_cartesian_product_in_fixed_order() {
        let c = Config::parse(
            "scenario = \"x\"\nseeds = 2\n[params]\nn = 5\n[sweep]\nb = [1, 2]\na = [\"p\", \"q\"]\n",
        )
        .unwrap();
        let pts = c.points();
        assert_eq!(pts.len(), 4);
        let order: Vec<(String, i64)> = pts
            .iter()
            .map(|p| (p.str("a").unwrap().to_string(), p.u64("b").unwrap() as i64))
            .collect();
        assert_eq!(
            order,
            vec![
                ("p".into(), 1),
                ("p".into(), 2),
                ("q".into(), 1),
                ("q".into(), 2)
            ]
        );
        assert_eq!(pts[0].u64("n").unwrap(), 5);
    }
}
