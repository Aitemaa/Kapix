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
        Self { cap, items: VecDeque::with_capacity(cap) }
    }
    /// Push newest; evicts the oldest beyond capacity.
    pub fn push(&mut self, c: RecentCapture) {
        todo!()
    }
    /// Newest first.
    pub fn iter(&self) -> impl Iterator<Item = &RecentCapture> {
        self.items.iter().rev()
    }
    /// Index 0 = newest.
    pub fn get(&self, i: usize) -> Option<&RecentCapture> {
        todo!()
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
