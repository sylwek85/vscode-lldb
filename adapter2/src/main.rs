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
extern crate env_logger;

use std::io;
use std::net;
use std::thread;
use std::time::Duration;

mod debug_session;
mod wire_protocol;
mod must_initialize;
mod worker_thread;

fn main() {
    env_logger::init();

    let listener = net::TcpListener::bind("127.0.0.1:4711").unwrap();
    let addr = listener.local_addr().unwrap();
    println!("Listening on port {}", addr.port());
    let (conn, addr) = listener.accept().unwrap();
    conn.set_nodelay(true);
    let conn_write = conn.try_clone().unwrap();

    let (debug_server, recv_message, send_message) =
        wire_protocol::DebugServer::new(Box::new(io::BufReader::new(conn)), Box::new(conn_write));

    let mut session = debug_session::DebugSession::new(send_message);
    loop {
        let message = recv_message.recv().unwrap();
        session.handle_message(message);
    }
}
