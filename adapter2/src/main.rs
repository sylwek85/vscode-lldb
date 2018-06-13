#![allow(unused)]

#[macro_use]
extern crate serde_derive;

extern crate debugserver_types;
extern crate lldb;
extern crate serde;
extern crate serde_json;

use std::io;
use std::net;
use std::thread;

mod debug_protocol;
mod wire_protocol;

fn main() {
    let listener = net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    println!("Listening on port {}", addr.port());
    let (conn, addr) = listener.accept().unwrap();
    conn.set_nodelay(true);
    let conn2 = conn.try_clone().unwrap();

    let (debug_server, recv_message, send_message) =
        wire_protocol::DebugServer::new(Box::new(io::BufReader::new(conn)), Box::new(conn2));

    let message = recv_message.recv().unwrap();
    loop {
        println!("###");
        thread::sleep_ms(1000);
    }
}
