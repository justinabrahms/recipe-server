//! In-memory LRU for stored shopping lists, keyed by short random tokens.
//! Per PLAN.md §6.3: 256 entries, 1-hour TTL.

use crate::shopping::Selection;
use lru::LruCache;
use rand::{distributions::Alphanumeric, Rng};
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CAP: usize = 256;
const TTL: Duration = Duration::from_secs(60 * 60);
const TOKEN_LEN: usize = 12;

#[derive(Clone)]
pub struct StoredList {
    pub selections: Vec<Selection>,
    pub created: Instant,
}

pub struct ListStore {
    inner: Mutex<LruCache<String, StoredList>>,
}

impl ListStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(LruCache::new(NonZeroUsize::new(CAP).unwrap())),
        }
    }

    pub fn put(&self, selections: Vec<Selection>) -> String {
        let token = mint_token();
        let mut g = self.inner.lock().unwrap();
        g.put(
            token.clone(),
            StoredList {
                selections,
                created: Instant::now(),
            },
        );
        token
    }

    pub fn get(&self, token: &str) -> Option<StoredList> {
        let mut g = self.inner.lock().unwrap();
        let v = g.get(token).cloned();
        match v {
            Some(stored) if stored.created.elapsed() < TTL => Some(stored),
            Some(_) => {
                g.pop(token);
                None
            }
            None => None,
        }
    }
}

impl Default for ListStore {
    fn default() -> Self {
        Self::new()
    }
}

fn mint_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(TOKEN_LEN)
        .map(char::from)
        .collect()
}
