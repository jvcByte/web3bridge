//! Week 5 · Day 4 — the shared client registry.
//!
//! The broadcast channel in `main.rs` moves *room* traffic. It cannot answer
//! "who is connected?", "is this nickname free?", or "send this to bob and
//! nobody else", because those need shared *state* and a channel has none. So
//! there is exactly one shared, mutable thing in this server, and it lives here.
//!
//! It holds two things per client: the nickname, and an [`mpsc::Sender`] that is
//! that client's private outgoing queue. Nick bookkeeping is why the registry
//! exists at all; the queue is what makes a private message actually private.
//!
//! # Why `std::sync::Mutex` and not `tokio::sync::Mutex`
//!
//! The rule (Week 6 Day 1, arriving a couple of days early): reach for
//! `tokio::sync::Mutex` **only** when you must hold the lock across an `.await`.
//! Every operation below is a `HashMap` insert or lookup — pure computation, no
//! I/O, microseconds. The std mutex is both correct and faster here, and Tokio's
//! own documentation says to prefer it for exactly this case.
//!
//! The invariant that makes it safe is that no method in this file `.await`s, so
//! no guard can possibly be alive across one. That is not a comment you should
//! trust on faith — check it. There is not an `async fn` in this module.
//!
//! # Why the cleanup lives in `Drop`
//!
//! A client can leave four ways: `/quit`, closing the socket, a network drop, or
//! a panic in its own task. Only the first is a path you would remember to write
//! cleanup for. [`ClientHandle`] removes the client from the registry in `Drop`,
//! so all four are handled by construction — including the panic, because
//! unwinding runs destructors.
//!
//! Forget this and `/who` slowly fills up with people who left. That bug is
//! invisible for the first ten minutes of a demo, which is what makes it worth a
//! deliberate design decision.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::mpsc::{self, error::TrySendError};

use crate::protocol::{validate_nick, Event, ParseError};

/// A connection's identity for the lifetime of that connection.
///
/// Deliberately not the nickname: nicknames change, and a message already in
/// flight must still be attributable to the client that sent it.
pub type ClientId = u64;

#[derive(Debug)]
pub enum NickError {
    /// The nickname broke a protocol rule — see [`validate_nick`].
    Invalid(ParseError),
    /// Somebody else already has it.
    Taken(String),
    /// The client is not registered. Unreachable through [`ClientHandle`]; it
    /// exists so a bug cannot turn into a panic that poisons the mutex.
    NotConnected,
}

impl std::fmt::Display for NickError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NickError::Invalid(e) => write!(f, "{e}"),
            NickError::Taken(nick) => write!(f, "nickname {nick:?} is already taken"),
            NickError::NotConnected => write!(f, "you are not connected"),
        }
    }
}

impl std::error::Error for NickError {}

/// What happened to a directed message.
///
/// Returned rather than swallowed, because the *sender* of a private message
/// deserves to know it did not arrive. A chat server that silently drops DMs is
/// worse than one that refuses them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    /// The recipient's queue is full — they are connected but not keeping up.
    /// The message is gone; we do not wait, because waiting here would mean one
    /// slow reader stalling whoever is sending to them.
    Backlogged,
    /// No such client, or their connection is already tearing down.
    Gone,
}

#[derive(Debug)]
struct Client {
    nick: String,
    /// This client's private outgoing queue, drained by its own write task.
    ///
    /// Bounded on purpose. An unbounded queue turns a client that stopped
    /// reading into unbounded server memory, which is a denial-of-service
    /// primitive handed to anyone who can open a socket.
    outbox: mpsc::Sender<Event>,
}

#[derive(Debug, Default)]
struct Inner {
    next_id: ClientId,
    by_id: HashMap<ClientId, Client>,
    /// lowercased nickname → id. A second index, kept in step with the first,
    /// so `/msg Alice` finds `alice`. Two maps means two chances to forget an
    /// update — every mutation below touches both.
    by_nick: HashMap<String, ClientId>,
}

#[derive(Debug, Default)]
pub struct Registry {
    inner: Mutex<Inner>,
}

impl Registry {
    pub fn new() -> Arc<Registry> {
        Arc::new(Registry::default())
    }

    /// Takes the lock, recovering from poisoning rather than propagating it.
    ///
    /// A `Mutex` is poisoned when a thread panics while holding it. The default
    /// `lock().unwrap()` would then make *every* later lock panic too — one
    /// client's bug takes down the whole server. Nothing in this module can
    /// leave `Inner` half-updated (each method finishes its map edits before
    /// returning), so ignoring the poison flag is sound here.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Registers a new connection and returns the guard that owns its lifetime.
    ///
    /// The default nickname embeds the id, so it is unique by construction — no
    /// retry loop, and a client is addressable the instant it connects.
    pub fn connect(self: &Arc<Self>, outbox: mpsc::Sender<Event>) -> ClientHandle {
        let id = {
            let mut inner = self.lock();
            let id = inner.next_id;
            inner.next_id += 1;

            let nick = format!("guest-{id}");
            inner.by_nick.insert(nick.to_lowercase(), id);
            inner.by_id.insert(id, Client { nick, outbox });
            id
            // The guard drops here, at the end of this block, before we touch
            // the Arc. Keeping critical sections visibly small is the habit;
            // it is what makes "no await inside the lock" easy to verify.
        };

        ClientHandle {
            registry: Arc::clone(self),
            id,
        }
    }

    /// Renames a client, returning the previous nickname on success.
    ///
    /// The check and the insert happen in **one** critical section. Split them
    /// into `if registry.is_free(n) { registry.set(n) }` and two clients running
    /// `/nick alice` at the same moment can both pass the check before either
    /// inserts — a textbook time-of-check-to-time-of-use race, and the one worth
    /// pointing at during review because the buggy version passes every
    /// single-threaded test.
    pub fn set_nick(&self, id: ClientId, new: &str) -> Result<String, NickError> {
        // Validation needs no lock, so it happens outside one.
        validate_nick(new).map_err(NickError::Invalid)?;

        let key = new.to_lowercase();
        let mut inner = self.lock();

        // Taken by somebody else. Taken by *you* is fine — it lets `alice`
        // become `Alice` without having to give up the name first.
        if let Some(&owner) = inner.by_nick.get(&key) {
            if owner != id {
                return Err(NickError::Taken(new.to_string()));
            }
        }

        let client = match inner.by_id.get_mut(&id) {
            Some(client) => client,
            None => return Err(NickError::NotConnected),
        };

        let old = std::mem::replace(&mut client.nick, new.to_string());

        inner.by_nick.remove(&old.to_lowercase());
        inner.by_nick.insert(key, id);

        Ok(old)
    }

    pub fn nick(&self, id: ClientId) -> Option<String> {
        self.lock().by_id.get(&id).map(|c| c.nick.clone())
    }

    /// Case-insensitive lookup, so `/msg ALICE hi` reaches `alice`.
    pub fn id_of(&self, nick: &str) -> Option<ClientId> {
        self.lock().by_nick.get(&nick.to_lowercase()).copied()
    }

    /// Queues an event for one specific client.
    ///
    /// Note the shape: the sender is **cloned out and the lock released** before
    /// anything is sent. `try_send` does not await, so holding the guard would
    /// be safe today — but the day somebody changes this to `send().await` to
    /// "fix" the `Backlogged` case, the lock would silently start being held
    /// across a yield point and the whole server would serialise behind the
    /// slowest client. Writing it this way means that edit cannot introduce the
    /// bug.
    pub fn deliver(&self, id: ClientId, event: Event) -> Delivery {
        let outbox = match self.lock().by_id.get(&id) {
            Some(client) => client.outbox.clone(),
            None => return Delivery::Gone,
        };

        match outbox.try_send(event) {
            Ok(()) => Delivery::Sent,
            Err(TrySendError::Full(_)) => Delivery::Backlogged,
            Err(TrySendError::Closed(_)) => Delivery::Gone,
        }
    }

    /// Every connected nickname, sorted so `/who` output is stable.
    ///
    /// Returns an owned `Vec` rather than anything borrowed from inside the
    /// lock — a method handing out a reference into `Inner` would have to keep
    /// the guard alive, and then the caller decides how long the lock is held.
    pub fn list(&self) -> Vec<String> {
        let mut nicks: Vec<String> = self.lock().by_id.values().map(|c| c.nick.clone()).collect();
        nicks.sort_unstable();
        nicks
    }

    pub fn len(&self) -> usize {
        self.lock().by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes a client, returning the nickname it held. Idempotent.
    ///
    /// Dropping the stored `Client` drops its `outbox` sender, which is what
    /// makes the write task's `recv()` return `None` and lets that task end.
    /// Deregistering and shutting down the writer are the same action.
    fn remove(&self, id: ClientId) -> Option<String> {
        let mut inner = self.lock();
        let client = inner.by_id.remove(&id)?;
        inner.by_nick.remove(&client.nick.to_lowercase());
        Some(client.nick)
    }
}

/// A connected client's registration, owned by its connection task.
///
/// The only way to get one is [`Registry::connect`], and dropping it
/// deregisters. That makes "remove the client on disconnect" unconditional
/// instead of something the disconnect path has to remember.
#[derive(Debug)]
pub struct ClientHandle {
    registry: Arc<Registry>,
    id: ClientId,
}

impl ClientHandle {
    pub fn id(&self) -> ClientId {
        self.id
    }

    /// The current nickname. Falls back to the default form only if the handle
    /// somehow outlived its registration, which the `Drop` impl prevents.
    pub fn nick(&self) -> String {
        self.registry
            .nick(self.id)
            .unwrap_or_else(|| format!("guest-{}", self.id))
    }

    pub fn set_nick(&self, new: &str) -> Result<String, NickError> {
        self.registry.set_nick(self.id, new)
    }

    /// Queues an event for this client alone: a reply, an error, a confirmation.
    pub fn send(&self, event: Event) -> Delivery {
        self.registry.deliver(self.id, event)
    }
}

impl Drop for ClientHandle {
    fn drop(&mut self) {
        self.registry.remove(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note what most of these tests do *not* need: a runtime or a socket.
    // Keeping the shared state synchronous and I/O-free is what buys that, and
    // it is why this module is the easiest part of the server to test. Async is
    // not free — do not spread it further than it has to go.

    /// A client whose outbox nobody drains. Fine for nick bookkeeping; the
    /// receiver is returned so the caller can keep it alive, because dropping it
    /// closes the channel.
    fn outbox() -> (mpsc::Sender<Event>, mpsc::Receiver<Event>) {
        mpsc::channel(8)
    }

    #[test]
    fn connect_assigns_unique_default_nicks() {
        let registry = Registry::new();
        let (tx_a, _rx_a) = outbox();
        let (tx_b, _rx_b) = outbox();

        let a = registry.connect(tx_a);
        let b = registry.connect(tx_b);

        assert_ne!(a.id(), b.id());
        assert_ne!(a.nick(), b.nick());
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn set_nick_renames_and_returns_the_old_name() {
        let registry = Registry::new();
        let (tx, _rx) = outbox();
        let alice = registry.connect(tx);

        let old = alice.set_nick("alice").unwrap();
        assert_eq!(old, format!("guest-{}", alice.id()));
        assert_eq!(alice.nick(), "alice");
    }

    #[test]
    fn set_nick_rejects_a_taken_nick() {
        let registry = Registry::new();
        let (tx_a, _rx_a) = outbox();
        let (tx_b, _rx_b) = outbox();
        let a = registry.connect(tx_a);
        let b = registry.connect(tx_b);

        a.set_nick("alice").unwrap();
        assert!(matches!(b.set_nick("alice"), Err(NickError::Taken(_))));

        // Case matters for display but not for uniqueness: two people called
        // "alice" and "Alice" is a phishing vector, not a feature.
        assert!(matches!(b.set_nick("ALICE"), Err(NickError::Taken(_))));
    }

    #[test]
    fn set_nick_rejects_an_invalid_nick() {
        let registry = Registry::new();
        let (tx, _rx) = outbox();
        let client = registry.connect(tx);

        // Forging a system notice: the protocol-level injection bug.
        assert!(matches!(client.set_nick("*admin"), Err(NickError::Invalid(_))));
        assert!(matches!(client.set_nick(""), Err(NickError::Invalid(_))));
    }

    #[test]
    fn renaming_frees_the_previous_nick() {
        let registry = Registry::new();
        let (tx_a, _rx_a) = outbox();
        let (tx_b, _rx_b) = outbox();
        let a = registry.connect(tx_a);
        let b = registry.connect(tx_b);

        a.set_nick("alice").unwrap();
        a.set_nick("alicia").unwrap();

        // If `set_nick` forgot to remove the old key from `by_nick`, this fails
        // and "alice" is unusable for the rest of the server's life.
        assert!(b.set_nick("alice").is_ok());
    }

    #[test]
    fn you_may_change_the_case_of_your_own_nick() {
        let registry = Registry::new();
        let (tx, _rx) = outbox();
        let client = registry.connect(tx);

        client.set_nick("alice").unwrap();
        assert!(client.set_nick("Alice").is_ok());
        assert_eq!(client.nick(), "Alice");
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let registry = Registry::new();
        let (tx, _rx) = outbox();
        let client = registry.connect(tx);
        client.set_nick("Alice").unwrap();

        assert_eq!(registry.id_of("alice"), Some(client.id()));
        assert_eq!(registry.id_of("ALICE"), Some(client.id()));
        assert_eq!(registry.id_of("bob"), None);
    }

    /// The ghost-user test. Without the `Drop` impl, `/who` grows forever.
    #[test]
    fn dropping_the_handle_deregisters_the_client() {
        let registry = Registry::new();
        let (tx, _rx) = outbox();
        let alice = registry.connect(tx);
        alice.set_nick("alice").unwrap();
        assert_eq!(registry.len(), 1);

        drop(alice);

        assert!(registry.is_empty(), "a departed client must leave no trace");
        assert_eq!(registry.id_of("alice"), None);
    }

    /// And the nickname must come free again, or a client that reconnects after
    /// a network blip cannot reclaim their own name.
    #[test]
    fn disconnecting_frees_the_nick_for_reuse() {
        let registry = Registry::new();

        let (tx_1, _rx_1) = outbox();
        let first = registry.connect(tx_1);
        first.set_nick("alice").unwrap();
        drop(first);

        let (tx_2, _rx_2) = outbox();
        let second = registry.connect(tx_2);
        assert!(second.set_nick("alice").is_ok());
    }

    #[test]
    fn list_is_sorted_so_who_output_is_stable() {
        let registry = Registry::new();
        let (tx_a, _rx_a) = outbox();
        let (tx_b, _rx_b) = outbox();
        let (tx_c, _rx_c) = outbox();
        let a = registry.connect(tx_a);
        let b = registry.connect(tx_b);
        let c = registry.connect(tx_c);

        c.set_nick("carol").unwrap();
        a.set_nick("alice").unwrap();
        b.set_nick("bob").unwrap();

        assert_eq!(registry.list(), vec!["alice", "bob", "carol"]);
    }

    #[tokio::test]
    async fn deliver_queues_to_the_right_client_only() {
        let registry = Registry::new();
        let (tx_a, mut rx_a) = outbox();
        let (tx_b, mut rx_b) = outbox();
        let alice = registry.connect(tx_a);
        let _bob = registry.connect(tx_b);

        assert_eq!(
            registry.deliver(alice.id(), Event::notice("just for you")),
            Delivery::Sent
        );

        assert_eq!(rx_a.recv().await, Some(Event::notice("just for you")));
        // Bob's queue never saw the bytes at all — this is what a per-client
        // channel buys over filtering a broadcast on arrival.
        assert!(rx_b.try_recv().is_err());
    }

    #[test]
    fn deliver_to_a_departed_client_reports_gone() {
        let registry = Registry::new();
        let (tx, _rx) = outbox();
        let alice = registry.connect(tx);
        let id = alice.id();
        drop(alice);

        assert_eq!(registry.deliver(id, Event::notice("hello?")), Delivery::Gone);
    }

    /// The per-client analogue of `broadcast`'s `Lagged`: a bounded queue that
    /// fills up because its owner is not reading.
    #[test]
    fn deliver_reports_backlogged_rather_than_blocking() {
        let registry = Registry::new();
        let (tx, _rx) = mpsc::channel(2);
        let slow = registry.connect(tx);

        assert_eq!(slow.send(Event::notice("1")), Delivery::Sent);
        assert_eq!(slow.send(Event::notice("2")), Delivery::Sent);

        // Full. The important part is that this returns instead of waiting —
        // whoever is sending must not be punished for the recipient being slow.
        assert_eq!(slow.send(Event::notice("3")), Delivery::Backlogged);
    }
}
