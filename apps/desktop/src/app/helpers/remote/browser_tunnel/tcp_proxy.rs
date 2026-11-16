//! The local SOCKS proxy a remote computer's Browser tabs use: the computer's own and
//! private-network addresses go through the remote route (its gxserver's authenticated TCP
//! connector, or SSH's dynamic forward), every other address opens from this computer
//! (CDXC:Browser 2026-11-14 in `../browser_route.rs`).
use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener},
    sync::Mutex,
    thread::JoinHandle,
    time::Duration,
};

use anyhow::{Context as _, Result, bail};
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::{
    Message, client::IntoClientRequest, http::HeaderValue, protocol::WebSocketConfig,
};

use crate::app::helpers::{GPUI_GXSERVER_PROTOCOL_VERSION, RemoteBrowserRoute};
use crate::app::model::GpuiRemoteGxserverRequestTarget;

const FRAME_BYTES: usize = 64 * 1024;
const CONNECTION_LIMIT: usize = 64;
/// How long opening a connection may take, here or through the remote route (an Easy Connect
/// relay answers its first byte in about a second).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// SOCKS5 replies (RFC 1928 section 6).
const REPLY_SUCCEEDED: u8 = 0;
const REPLY_GENERAL_FAILURE: u8 = 1;
const REPLY_NETWORK_UNREACHABLE: u8 = 3;
const REPLY_HOST_UNREACHABLE: u8 = 4;
const REPLY_CONNECTION_REFUSED: u8 = 5;

/// How an address the route sends to the remote computer gets there.
#[derive(Clone)]
pub(super) enum BrowserUpstream {
    /// The owning gxserver's authenticated TCP connector (a Windows computer).
    Gxserver(GpuiRemoteGxserverRequestTarget),
    /// A SOCKS5 server on this computer's loopback that reaches the computer: SSH's dynamic
    /// forward on this port.
    Socks(u16),
}

pub(super) struct BrowserTcpProxy {
    shutdown: watch::Sender<bool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl BrowserTcpProxy {
    pub(super) fn start(
        listener: TcpListener,
        upstream: BrowserUpstream,
        route: RemoteBrowserRoute,
    ) -> Result<Self> {
        listener.set_nonblocking(true)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let listener = {
            let _entered = runtime.enter();
            tokio::net::TcpListener::from_std(listener)?
        };
        let (shutdown, mut canceled) = watch::channel(false);
        let thread = std::thread::Builder::new()
            .name("remote-browser-tcp".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let mut connections = tokio::task::JoinSet::new();
                    loop {
                        tokio::select! {
                            _ = canceled.changed() => break,
                            _ = connections.join_next(), if !connections.is_empty() => {},
                            accepted = listener.accept() => {
                                let Ok((stream, _)) = accepted else { break };
                                if connections.len() >= CONNECTION_LIMIT {
                                    drop(stream);
                                    continue;
                                }
                                let upstream = upstream.clone();
                                let route = route.clone();
                                connections.spawn(async move {
                                    // Errors are returned to SOCKS callers; page traffic and auth stay out of logs.
                                    let _ = serve_connection(stream, upstream, route).await;
                                });
                            },
                        }
                    }
                    connections.abort_all();
                    while connections.join_next().await.is_some() {}
                });
            })?;
        Ok(Self {
            shutdown,
            thread: Mutex::new(Some(thread)),
        })
    }

    pub(super) fn stop(&self) {
        let _ = self.shutdown.send(true);
        if let Ok(mut thread) = self.thread.lock()
            && let Some(thread) = thread.take()
        {
            let _ = thread.join();
        }
    }

    pub(super) fn is_alive(&self) -> bool {
        self.thread
            .lock()
            .ok()
            .is_some_and(|thread| thread.as_ref().is_some_and(|thread| !thread.is_finished()))
    }
}

impl Drop for BrowserTcpProxy {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn socks_destination(stream: &mut TcpStream) -> Result<(String, u16)> {
    let mut greeting = [0; 2];
    stream.read_exact(&mut greeting).await?;
    if greeting[0] != 5 || greeting[1] == 0 {
        bail!("Invalid SOCKS greeting.");
    }
    let mut methods = vec![0; greeting[1] as usize];
    stream.read_exact(&mut methods).await?;
    if !methods.contains(&0) {
        stream.write_all(&[5, 255]).await?;
        bail!("SOCKS authentication method unavailable.");
    }
    stream.write_all(&[5, 0]).await?;
    let mut header = [0; 4];
    stream.read_exact(&mut header).await?;
    if header[0] != 5 || header[1] != 1 || header[2] != 0 {
        stream.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        bail!("Only SOCKS CONNECT is supported.");
    }
    let host = match header[3] {
        1 => {
            let mut bytes = [0; 4];
            stream.read_exact(&mut bytes).await?;
            IpAddr::V4(Ipv4Addr::from(bytes)).to_string()
        }
        4 => {
            let mut bytes = [0; 16];
            stream.read_exact(&mut bytes).await?;
            IpAddr::V6(Ipv6Addr::from(bytes)).to_string()
        }
        3 => {
            let length = stream.read_u8().await? as usize;
            let mut bytes = vec![0; length];
            stream.read_exact(&mut bytes).await?;
            String::from_utf8(bytes).context("Invalid SOCKS hostname.")?
        }
        _ => {
            stream.write_all(&[5, 8, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            bail!("Invalid SOCKS address type.");
        }
    };
    let port = stream.read_u16().await?;
    Ok((host, port))
}

async fn serve_connection(
    mut browser: TcpStream,
    upstream: BrowserUpstream,
    route: RemoteBrowserRoute,
) -> Result<()> {
    browser.set_nodelay(true)?;
    let (host, port) =
        tokio::time::timeout(Duration::from_secs(5), socks_destination(&mut browser)).await??;
    if !route.host_goes_through_machine(&host) {
        return serve_from_this_computer(browser, &host, port).await;
    }
    match upstream {
        BrowserUpstream::Gxserver(target) => {
            serve_through_gxserver(browser, &host, port, target).await
        }
        BrowserUpstream::Socks(socks_port) => {
            serve_through_socks(browser, &host, port, socks_port).await
        }
    }
}

/// Answers the browser's CONNECT with `reply` (no bound address).
async fn reply(browser: &mut TcpStream, reply: u8) -> io::Result<()> {
    browser.write_all(&[5, reply, 0, 1, 0, 0, 0, 0, 0, 0]).await
}

/// Opens `host:port` from this computer (its own DNS and network) and relays the bytes.
async fn serve_from_this_computer(mut browser: TcpStream, host: &str, port: u16) -> Result<()> {
    let mut destination =
        match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port))).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) => {
                let code = match error.kind() {
                    io::ErrorKind::ConnectionRefused => REPLY_CONNECTION_REFUSED,
                    io::ErrorKind::NetworkUnreachable => REPLY_NETWORK_UNREACHABLE,
                    _ => REPLY_HOST_UNREACHABLE,
                };
                reply(&mut browser, code).await?;
                bail!("The destination could not be reached from this computer.");
            }
            Err(_) => {
                reply(&mut browser, REPLY_HOST_UNREACHABLE).await?;
                bail!("The destination did not answer in time.");
            }
        };
    destination.set_nodelay(true)?;
    reply(&mut browser, REPLY_SUCCEEDED).await?;
    tokio::io::copy_bidirectional(&mut browser, &mut destination).await?;
    Ok(())
}

/// Opens `host:port` through the SOCKS5 server on this computer's `socks_port` (SSH's dynamic
/// forward to the remote computer, which resolves the name there) and relays the bytes.
async fn serve_through_socks(
    mut browser: TcpStream,
    host: &str,
    port: u16,
    socks_port: u16,
) -> Result<()> {
    let connect = async {
        let mut remote = TcpStream::connect((Ipv4Addr::LOCALHOST, socks_port)).await?;
        remote.set_nodelay(true)?;
        remote.write_all(&[5, 1, 0]).await?;
        let mut choice = [0; 2];
        remote.read_exact(&mut choice).await?;
        if choice != [5, 0] {
            bail!("The remote route refused the SOCKS greeting.");
        }
        let mut request = vec![5, 1, 0];
        match host.parse::<IpAddr>() {
            Ok(IpAddr::V4(ip)) => {
                request.push(1);
                request.extend(ip.octets());
            }
            Ok(IpAddr::V6(ip)) => {
                request.push(4);
                request.extend(ip.octets());
            }
            Err(_) => {
                let name = host.as_bytes();
                let length = u8::try_from(name.len()).context("SOCKS hostname too long.")?;
                request.push(3);
                request.push(length);
                request.extend(name);
            }
        }
        request.extend(port.to_be_bytes());
        remote.write_all(&request).await?;
        let mut header = [0; 4];
        remote.read_exact(&mut header).await?;
        let bound_length = match header[3] {
            1 => 4,
            4 => 16,
            3 => remote.read_u8().await? as usize,
            _ => bail!("Invalid SOCKS reply."),
        };
        let mut bound = vec![0; bound_length + 2];
        remote.read_exact(&mut bound).await?;
        Ok::<_, anyhow::Error>((remote, header[1]))
    };
    let mut remote = match tokio::time::timeout(CONNECT_TIMEOUT, connect).await {
        Ok(Ok((remote, REPLY_SUCCEEDED))) => remote,
        Ok(Ok((_, code))) => {
            reply(&mut browser, code).await?;
            bail!("The remote computer could not reach the destination.");
        }
        _ => {
            reply(&mut browser, REPLY_GENERAL_FAILURE).await?;
            bail!("The remote route did not answer.");
        }
    };
    reply(&mut browser, REPLY_SUCCEEDED).await?;
    tokio::io::copy_bidirectional(&mut browser, &mut remote).await?;
    Ok(())
}

/// Opens `host:port` through the remote gxserver's authenticated TCP connector and relays the
/// bytes as WebSocket frames.
async fn serve_through_gxserver(
    mut browser: TcpStream,
    host: &str,
    port: u16,
    target: GpuiRemoteGxserverRequestTarget,
) -> Result<()> {
    let connect = async {
        let mut url = gpui::http_client::Url::parse(&format!(
            "ws://127.0.0.1:{}/api/browserTcp",
            target.local_port
        ))?;
        url.query_pairs_mut()
            .append_pair(
                "protocolVersion",
                &GPUI_GXSERVER_PROTOCOL_VERSION.to_string(),
            )
            .append_pair("host", host)
            .append_pair("port", &port.to_string());
        ghostex_paths::loopback_owner::check_host_port("127.0.0.1", target.local_port)?;
        let mut request = url.as_str().into_client_request()?;
        request.headers_mut().insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", target.token))?,
        );
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, target.local_port)).await?;
        stream.set_nodelay(true)?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(FRAME_BYTES))
            .max_frame_size(Some(FRAME_BYTES));
        let (socket, _) =
            tokio_tungstenite::client_async_with_config(request, stream, Some(config)).await?;
        Ok::<_, anyhow::Error>(socket)
    };
    let socket = match tokio::time::timeout(Duration::from_secs(12), connect).await {
        Ok(Ok(socket)) => socket,
        _ => {
            reply(&mut browser, REPLY_CONNECTION_REFUSED).await?;
            bail!("The remote browser destination could not be reached.");
        }
    };
    reply(&mut browser, REPLY_SUCCEEDED).await?;
    let (mut browser_rx, mut browser_tx) = browser.into_split();
    let (mut remote_tx, mut remote_rx) = socket.split();
    let upload = async {
        let mut buffer = [0; 32 * 1024];
        loop {
            let count = browser_rx.read(&mut buffer).await?;
            if count == 0 {
                remote_tx.send(Message::Text("eof".into())).await?;
                return Ok::<(), anyhow::Error>(());
            }
            remote_tx
                .send(Message::Binary(buffer[..count].to_vec().into()))
                .await?;
        }
    };
    let download = async {
        while let Some(message) = remote_rx.next().await {
            match message? {
                Message::Binary(bytes) => browser_tx.write_all(&bytes).await?,
                Message::Text(text) if text.as_str() == "eof" => {
                    browser_tx.shutdown().await?;
                    return Ok::<(), anyhow::Error>(());
                }
                Message::Ping(_) | Message::Pong(_) => {}
                _ => return Err(io::Error::from(io::ErrorKind::ConnectionAborted).into()),
            }
        }
        Err(io::Error::from(io::ErrorKind::ConnectionAborted).into())
    };
    tokio::try_join!(upload, download)?;
    Ok(())
}
