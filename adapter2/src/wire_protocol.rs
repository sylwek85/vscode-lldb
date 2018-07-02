use debug_protocol::ProtocolMessage;
use serde_json::{self, Value};
use std::error::Error;
use std::io;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread;
use std::str;

pub struct DebugServer {
    sender: thread::JoinHandle<()>,
    receiver: thread::JoinHandle<()>,
}

impl DebugServer {
    pub fn new(
        mut reader: Box<io::BufRead + Send>, mut writer: Box<io::Write + Send>,
    ) -> (Self, Receiver<ProtocolMessage>, SyncSender<ProtocolMessage>) {
        let (inbound_send, inbound_recv) = sync_channel::<ProtocolMessage>(100);
        let (outbound_send, outbound_recv) = sync_channel::<ProtocolMessage>(100);
        let outbound_send2 = outbound_send.clone();

        let receiver = thread::spawn(move || {
            let mut buffer: Vec<u8> = vec![];
            let mut line = String::new();
            loop {
                line.clear();
                reader.read_line(&mut line);
                if line.starts_with("Content-Length:") {
                    let content_len = line[15..].trim().parse::<usize>().unwrap();
                    line.clear();
                    reader.read_line(&mut line);
                    buffer.resize(content_len, 0);
                    reader.read_exact(&mut buffer).unwrap();
                    println!("rx: {}", str::from_utf8(&buffer).unwrap());
                    match serde_json::from_slice(&buffer) {
                        Ok(message) => {
                            inbound_send.send(message);
                        }
                        Err(err) => {
                            if (err.is_data()) {
                                // Try reading as generic JSON value.
                                if let Ok(msg) = serde_json::from_slice::<Value>(&buffer) {
                                    inbound_send.send(ProtocolMessage::Unknown(msg));
                                }
                            }
                        }
                    }
                }
            }
        });

        let sender = thread::spawn(move || loop {
            let message = outbound_recv.recv().unwrap();
            let buffer = serde_json::to_vec(&message).unwrap();
            println!("tx: {}", str::from_utf8(&buffer).unwrap());
            writeln!(&mut writer, "Content-Length:{}", buffer.len());
            writeln!(&mut writer, "");
            writer.write(&buffer);
        });

        let debug_server = DebugServer { sender, receiver };

        (debug_server, inbound_recv, outbound_send)
    }
}
