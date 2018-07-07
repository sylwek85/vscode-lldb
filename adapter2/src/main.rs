#![allow(unused)]
#![feature(try_trait)]
#![feature(fnbox)]

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate failure_derive;
extern crate debug_protocol;
extern crate failure;
extern crate my_lldb as lldb;
#[macro_use]
extern crate log;
extern crate bytes;
extern crate env_logger;

extern crate futures;
extern crate tokio;
extern crate tokio_codec;
extern crate tokio_io;
extern crate tokio_threadpool;

use futures::prelude::*;
use tokio::prelude::*;

use futures::future::{lazy, poll_fn};
use futures::sync::mpsc;
use tokio::io;
use tokio::net::TcpListener;
use tokio_codec::Decoder;
use tokio_threadpool::blocking;

mod debug_session;
mod must_initialize;
mod wire_protocol;
mod worker_thread;

fn main() {
    env_logger::init();
    let addr = "127.0.0.1:4711".parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();
    println!("Listening on port {}", addr.port());

    let server = listener
        .incoming()
        .for_each(|conn| {
            conn.set_nodelay(true);
            let (tx_frame, rx_frame) = wire_protocol::Codec::new().framed(conn).split();

            let (tx_message, rx_message) = mpsc::channel(10);
            let mut session = debug_session::DebugSession::new(tx_message);

            let send_pipe = rx_message
                .map_err(|_| io::Error::from(io::ErrorKind::Other))
                .forward(tx_frame);

            let recv_pipe = rx_frame
                .for_each(move |message| {
                    blocking(|| session.handle_message(message)).map_err(|_| panic!("the threadpool shut down"));
                    Ok(())
                })
                .map_err(|_| io::Error::from(io::ErrorKind::Other));

            let hren = send_pipe.join(recv_pipe);

            Ok(())
        })
        .then(|_| Ok(()));
    // .map_err(|err| {
    //     error!("accept error: {:?}", err);
    // })
    // .map(|_| ());

    tokio::run(server);
}
