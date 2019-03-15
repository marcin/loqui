use failure::Error;
use std::net::SocketAddr;

use tokio::await;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio_codec::Framed;

use crate::protocol::codec::{LoquiCodec, LoquiFrame};
use crate::protocol::frames::*;

pub async fn run<A: AsRef<str>>(address: A) -> Result<(), Error> {
    let addr: SocketAddr = address.as_ref().parse()?;
    let listener = TcpListener::bind(&addr)?;
    println!("Starting {:?} ...", address.as_ref());
    let mut incoming = listener.incoming();
    loop {
        match await!(incoming.next()) {
            Some(Ok(tcp_stream)) => {
                tokio::spawn_async(handle_connection(tcp_stream));
            }
            other => {
                println!("incoming.next() return odd result. {:?}", other);
            }
        }
    }
    Ok(())
}

fn handle_frame(frame: LoquiFrame) -> Option<LoquiFrame> {
    match frame {
        LoquiFrame::Request(Request {
            flags,
            sequence_id,
            payload,
            ..
        }) => Some(LoquiFrame::Response(Response {
            flags,
            sequence_id,
            payload,
        })),
        LoquiFrame::Hello(Hello {
            flags,
            version,
            encodings,
            compressions,
        }) => Some(LoquiFrame::HelloAck(HelloAck {
            flags,
            ping_interval_ms: 100,
            encoding: "".to_string(),
            compression: "".to_string(),
        })),
        frame => {
            info!("unhandled frame {:?}", frame);
            None
        }
    }
}

async fn upgrade(mut socket: TcpStream) -> TcpStream {
    // TODO: buffering
    let mut payload = [0; 1024];
    while let Ok(bytes_read) = await!(socket.read_async(&mut payload)) {
        let request = String::from_utf8(payload.to_vec()).unwrap();
        // TODO: better
        if request.contains(&"upgrade") || request.contains(&"Upgrade") {
            let response =
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: loqui\r\nConnection: Upgrade\r\n\r\n";
            await!(socket.write_all_async(&response.as_bytes()[..])).unwrap();
            await!(socket.flush_async()).unwrap();
            break;
        }
    }
    socket
}

async fn handle_connection(mut socket: TcpStream) {
    socket = await!(upgrade(socket));
    let framed_socket = Framed::new(socket, LoquiCodec::new(50000));
    let (mut writer, mut reader) = framed_socket.split();
    // TODO: handle disconnect, bytes_read=0
    while let Some(result) = await!(reader.next()) {
        match result {
            Ok(frame) => {
                if let Some(response) = handle_frame(frame) {
                    match await!(writer.send(response)) {
                        Ok(new_writer) => writer = new_writer,
                        // TODO: better handle this error
                        Err(e) => {
                            error!("Failed to write. error={:?}", e);
                            return;
                        }
                    }
                }
            }
            Err(e) => {
                dbg!(e);
            }
        }
    }
}