//! Data threads that need no terminal: the OpenCode poller loop (`tasks/T34-opencode-client.md`, a round
//! every second since `tasks/T42`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::app::Event;
use crate::app::run::sleep;
use crate::collect::opencode::OpenCodePoller;
use crate::http::Client;

/// Polls the OpenCode server every second for the session in `focus` until `stop` is set or the UI is gone.
pub fn opencode_loop(
    client: Client,
    tx: Sender<Event>,
    stop: Arc<AtomicBool>,
    focus: Arc<Mutex<Option<String>>>,
) {
    let mut poller = OpenCodePoller::new(client);
    while !stop.load(Ordering::Relaxed) {
        let focused = focus.lock().unwrap().clone();
        if tx.send(Event::OpenCode(Box::new(poller.poll(focused.as_deref())))).is_err() {
            break;
        }
        sleep(&stop, 1000);
    }
}
