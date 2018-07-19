#![allow(unused)]

#![feature(try_trait)]
#![feature(fnbox)]
#![feature(nll)]

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate failure_derive;
extern crate debug_protocol;
extern crate failure;
extern crate lldb;
#[macro_use]
extern crate log;
extern crate bytes;
extern crate env_logger;
extern crate regex;
extern crate globset;

extern crate futures;
extern crate tokio;
extern crate tokio_codec;
extern crate tokio_io;
extern crate tokio_threadpool;

use std::mem;

use futures::prelude::*;
use tokio::prelude::*;

use futures::future::{lazy, poll_fn};
use futures::sync::mpsc;
use tokio::io;
use tokio::net::TcpListener;
use tokio_codec::Decoder;
use tokio_threadpool::blocking;

use lldb::*;

mod cancellation;
mod debug_session;
mod handles;
mod must_initialize;
mod wire_protocol;
mod launch_config;

fn main() {
    env_logger::init();
    SBDebugger::initialize();

    let addr = "127.0.0.1:4711".parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();
    println!("Listening on port {}", addr.port());

    let server = listener
        .incoming()
        .map_err(|err| {
            error!("accept error: {:?}", err);
            panic!()
        })
        .take(1)
        .for_each(|conn| {
            conn.set_nodelay(true);
            let (to_client, from_client) = wire_protocol::Codec::new().framed(conn).split();
            let (to_session, from_session) = debug_session::DebugSession::new().split();

            let client_to_session = from_client
                .map_err(|_| ())
                .forward(to_session)
                .then(|r| {info!("### client_to_session resolved"); Ok(())} );
            tokio::spawn(client_to_session);

            let session_to_client = from_session
                .map_err(|err| io::Error::new(io::ErrorKind::Other, "DebugSession error"))
                .forward(to_client)
                .then(|r| {info!("### session_to_client resolved"); Ok(())});

            session_to_client
        })
    .then(|r| {info!("### server resolved {:?}", r); Ok(()) });

    tokio::run(server);

    info!("Exited");

    SBDebugger::terminate();
}
