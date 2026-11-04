//! `ghostex box relay --listen <addr> --to <addr>`: a plain TCP forwarder. The `local` provider
//! runs it in the container so Docker can publish the box gxserver (which listens only on the
//! container's loopback) on this computer's loopback; real providers use tailcat instead.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread;

fn pump(mut from: TcpStream, mut to: TcpStream) {
    let mut buffer = [0u8; 32 * 1024];
    loop {
        match from.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                if to.write_all(&buffer[..read]).is_err() {
                    break;
                }
            }
        }
    }
    let _ = to.shutdown(Shutdown::Write);
}

/// Serves forever.
pub(crate) fn run_relay(listen: &str, target: &str) -> Result<(), String> {
    let listener =
        TcpListener::bind(listen).map_err(|error| format!("Can't listen on {listen}: {error}"))?;
    for incoming in listener.incoming() {
        let Ok(client) = incoming else { continue };
        let target = target.to_string();
        thread::spawn(move || {
            let Ok(upstream) = TcpStream::connect(&target) else {
                return;
            };
            let (Ok(client_read), Ok(upstream_read)) = (client.try_clone(), upstream.try_clone())
            else {
                return;
            };
            let forward = thread::spawn(move || pump(client_read, upstream));
            pump(upstream_read, client);
            let _ = forward.join();
        });
    }
    Ok(())
}
