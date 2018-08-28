use std::sync::{Arc, Mutex};
use std::thread;

use futures::prelude::*;
use futures::stream;
use futures::sync::mpsc;
use futures::sync::oneshot;
use tokio;
use tokio::prelude::*;
use tokio_threadpool::blocking;

use super::*;
use crate::cancellation::{CancellationSource, CancellationToken};
use crate::debug_protocol::*;
use crate::must_initialize::MustInitialize;

pub struct DebugSessionTokio {
    inner: Arc<Mutex<DebugSession>>,
    sender_in: mpsc::Sender<ProtocolMessage>,
    receiver_out: mpsc::Receiver<ProtocolMessage>,
    shutdown_token: CancellationToken,
}

unsafe impl Send for DebugSessionTokio {}

impl Drop for DebugSessionTokio {
    fn drop(&mut self) {
        info!("### Dropping DebugSessionTokio");
    }
}

impl DebugSessionTokio {
    pub fn new() -> Self {
        let (sender_in, receiver_in) = mpsc::channel(10);
        let (sender_out, receiver_out) = mpsc::channel(10);
        let shutdown = CancellationSource::new();
        let shutdown_token = shutdown.cancellation_token();

        let inner = Arc::new(Mutex::new(DebugSession::new(sender_out, shutdown)));
        let weak_inner = Arc::downgrade(&inner);
        inner.lock().unwrap().self_ref = MustInitialize::Initialized(weak_inner);

        // Dispatch incoming requests to inner.handle_message()
        let inner2 = inner.clone();
        let sink_to_inner = receiver_in
            .for_each(move |msg: ProtocolMessage| {
                let inner2 = inner2.clone();
                future::poll_fn(move || {
                    let msg = msg.clone();
                    blocking(|| {
                        inner2.lock().unwrap().handle_message(msg);
                    })
                }).map_err(|_| ())
            }).then(|r| {
                info!("### sink_to_inner resolved {:?}", r);
                r
            });
        tokio::spawn(sink_to_inner);

        // Create a thread listening on inner's event_listener
        let (mut sender, mut receiver) = mpsc::channel(100);
        let listener = inner.lock().unwrap().event_listener.clone();
        let token2 = shutdown_token.clone();
        thread::spawn(move || {
            let mut event = SBEvent::new();
            while !token2.is_cancelled() {
                if listener.wait_for_event(1, &mut event) {
                    match sender.try_send(event) {
                        Err(err) => error!("Could not send event to DebugSession: {:?}", err),
                        Ok(_) => {}
                    }
                    event = SBEvent::new();
                }
            }
            info!("cancelled?");
        });
        // Dispatch incoming events to inner.handle_debug_event()
        let inner2 = inner.clone();
        let event_listener_to_inner = receiver
            .for_each(move |event| {
                inner2.lock().unwrap().handle_debug_event(event);
                Ok(())
            }).then(|r| {
                info!("### event_listener_to_inner resolved: {:?}", r);
                r
            });
        tokio::spawn(event_listener_to_inner);

        DebugSessionTokio {
            inner,
            sender_in,
            receiver_out,
            shutdown_token,
        }
    }
}

impl Stream for DebugSessionTokio {
    type Item = ProtocolMessage;
    type Error = ();
    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.receiver_out.poll() {
            Ok(Async::NotReady) if self.shutdown_token.is_cancelled() => {
                error!("Stream::poll after shutdown");
                Ok(Async::Ready(None))
            }
            Ok(r) => Ok(r),
            Err(e) => Err(e),
        }
    }
}

impl Sink for DebugSessionTokio {
    type SinkItem = ProtocolMessage;
    type SinkError = ();
    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.shutdown_token.is_cancelled() {
            error!("Sink::start_send after shutdown");
            Err(())
        } else {
            self.sender_in.start_send(item).map_err(|err| panic!("{:?}", err))
        }
    }
    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        if self.shutdown_token.is_cancelled() {
            error!("Sink::poll_complete after shutdown");
            Err(())
        } else {
            self.sender_in.poll_complete().map_err(|err| panic!("{:?}", err))
        }
    }
}
