//! Discrete-event core: virtual time and a binary-heap event queue. Events at
//! equal times pop in insertion order (a sequence number breaks ties), so a
//! run is deterministic given its RNG stream.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

struct Entry<E> {
    t: f64,
    seq: u64,
    ev: E,
}

impl<E> PartialEq for Entry<E> {
    fn eq(&self, o: &Self) -> bool {
        self.t.total_cmp(&o.t) == Ordering::Equal && self.seq == o.seq
    }
}
impl<E> Eq for Entry<E> {}
impl<E> PartialOrd for Entry<E> {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl<E> Ord for Entry<E> {
    // Reversed: BinaryHeap is a max-heap and we want the earliest event.
    fn cmp(&self, o: &Self) -> Ordering {
        o.t.total_cmp(&self.t).then(o.seq.cmp(&self.seq))
    }
}

pub struct Engine<E> {
    now: f64,
    seq: u64,
    q: BinaryHeap<Entry<E>>,
}

impl<E> Default for Engine<E> {
    fn default() -> Self {
        Engine {
            now: 0.0,
            seq: 0,
            q: BinaryHeap::new(),
        }
    }
}

impl<E> Engine<E> {
    pub fn now(&self) -> f64 {
        self.now
    }

    /// Schedule `ev` at absolute time `t` (never in the past).
    pub fn at(&mut self, t: f64, ev: E) {
        assert!(t >= self.now, "event scheduled in the past");
        self.seq += 1;
        self.q.push(Entry {
            t,
            seq: self.seq,
            ev,
        });
    }

    pub fn after(&mut self, dt: f64, ev: E) {
        self.at(self.now + dt.max(0.0), ev);
    }

    /// Pop the next event if it is at or before `horizon`, advancing time.
    pub fn next(&mut self, horizon: f64) -> Option<E> {
        if self.q.peek().is_some_and(|e| e.t <= horizon) {
            let e = self.q.pop().expect("peeked");
            self.now = e.t;
            Some(e.ev)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_by_time_then_insertion() {
        let mut e = Engine::default();
        e.at(2.0, "c");
        e.at(1.0, "a");
        e.at(1.0, "b");
        e.at(9.0, "late");
        let mut got = vec![];
        while let Some(x) = e.next(5.0) {
            got.push(x);
        }
        assert_eq!(got, vec!["a", "b", "c"]);
        assert_eq!(e.now(), 2.0);
    }
}
