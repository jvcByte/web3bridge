//! Shared server state — room map and name registry.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::broadcast;

pub const CHANNEL_CAPACITY: usize = 64;

// ---------------------------------------------------------------------------
// Per-room state
// ---------------------------------------------------------------------------

pub struct Room {
    pub tx: broadcast::Sender<String>,
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    pub rooms: Mutex<HashMap<String, Room>>,
    pub names: Mutex<HashSet<String>>,
    pub count: AtomicUsize,
}

impl AppState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            rooms: Mutex::new(HashMap::new()),
            names: Mutex::new(HashSet::new()),
            count: AtomicUsize::new(0),
        })
    }

    /// Returns the sender for `room`, creating it if it does not exist.
    pub fn get_or_create_room(&self, name: &str) -> broadcast::Sender<String> {
        let mut map = self.rooms.lock().unwrap();
        if let Some(r) = map.get(name) {
            if r.tx.receiver_count() > 0 {
                return r.tx.clone();
            }
        }
        let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
        map.insert(name.to_string(), Room { tx: tx.clone() });
        tx
    }

    /// Remove rooms with no subscribers.
    pub fn prune_rooms(&self) {
        self.rooms.lock().unwrap().retain(|_, r| r.tx.receiver_count() > 0);
    }

    /// Try to register `name`. Returns `true` on success, `false` if taken.
    pub fn register_name(&self, name: &str) -> bool {
        let mut names = self.names.lock().unwrap();
        if names.contains(name) {
            false
        } else {
            names.insert(name.to_string());
            true
        }
    }

    pub fn remove_name(&self, name: &str) {
        self.names.lock().unwrap().remove(name);
    }

    pub fn list_names(&self) -> Vec<String> {
        self.names.lock().unwrap().iter().cloned().collect()
    }

    pub fn connected(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }
}
