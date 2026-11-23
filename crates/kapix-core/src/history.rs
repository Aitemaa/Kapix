//! Recent captures ring buffer (last N, stored PNG-compressed to stay within RAM budgets).

use std::collections::VecDeque;

use crate::template::DateTime;

#[derive(Debug, Clone)]
pub struct RecentCapture {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub taken: DateTime,
}

#[derive(Debug, Clone)]
pub struct Recent {
    cap: usize,
    items: VecDeque<RecentCapture>,
}

impl Recent {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            items: VecDeque::with_capacity(cap),
        }
    }
    /// Push newest; evicts the oldest beyond capacity.
    pub fn push(&mut self, c: RecentCapture) {
        if self.cap == 0 {
            return;
        }
        while self.items.len() >= self.cap {
            self.items.pop_front();
        }
        self.items.push_back(c);
    }
    /// Newest first.
    pub fn iter(&self) -> impl Iterator<Item = &RecentCapture> {
        self.items.iter().rev()
    }
    /// Index 0 = newest.
    pub fn get(&self, i: usize) -> Option<&RecentCapture> {
        let idx = self.items.len().checked_sub(i + 1)?;
        self.items.get(idx)
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(n: u8) -> RecentCapture {
        RecentCapture {
            png: vec![n],
            width: 1,
            height: 1,
            taken: DateTime::default(),
        }
    }

    #[test]
    fn ring_evicts_oldest_and_orders_newest_first() {
        let mut r = Recent::new(3);
        assert!(r.is_empty() && r.get(0).is_none());
        for n in 1..=5 {
            r.push(cap(n));
        }
        assert_eq!(r.len(), 3);
        assert_eq!(
            r.iter().map(|c| c.png[0]).collect::<Vec<_>>(),
            vec![5, 4, 3]
        );
        assert_eq!(r.get(0).unwrap().png[0], 5);
        assert_eq!(r.get(2).unwrap().png[0], 3);
        assert!(r.get(3).is_none());
    }

    #[test]
    fn zero_capacity_stores_nothing() {
        let mut r = Recent::new(0);
        r.push(cap(1));
        assert!(r.is_empty());
    }
}
