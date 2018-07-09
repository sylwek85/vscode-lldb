use std::error;
use std::sync::Arc;
use std::thread;

use futures::prelude::*;
use futures::sync::mpsc;

use lldb::*;

pub struct EventListener {
    listener: Arc<SBListener>,
    thread: thread::JoinHandle<()>,
}

impl EventListener {
    pub fn new(name: &str) -> (EventListener, mpsc::Receiver<SBEvent>) {
        let listener = Arc::new(SBListener::new_with_name(name));
        let listener2 = listener.clone();
        let (mut sender, mut receiver) = mpsc::channel(10);
        let thread = thread::spawn(move || {
            let mut event = SBEvent::new();
            while sender.poll_ready().is_ok() {
                if listener.wait_for_event(1, &mut event) {
                    if sender.try_send(event).is_err() {
                        break;
                    }
                    event = SBEvent::new();
                }
            }
        });
        let event_listener = EventListener {
            listener: listener2,
            thread: thread
        };
        (event_listener, receiver)
    }
}

impl AsRef<SBListener> for EventListener {
    fn as_ref(&self) -> &SBListener {
        &self.listener
    }
}
