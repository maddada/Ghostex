//! Local SOCKS facade over the owning gxserver's authenticated TCP connector.
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

use crate::app::helpers::GPUI_GXSERVER_PROTOCOL_VERSION;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

const FRAME_BYTES: usize = 64 * 1024;
const CONNECTION_LIMIT: usize = 64;

pub(super) struct BrowserTcpProxy {
    shutdown: watch::Sender<bool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl BrowserTcpProxy {
    pub(super) fn start(
        listener: TcpListener,
        target: GpuiRemoteGxserverRequestTarget,
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
                                let target = target.clone();
                                connections.spawn(async move {
                                    // Errors are returned to SOCKS callers; page traffic and auth stay out of logs.
                                    let _ = serve_connection(stream, target).await;
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
    target: GpuiRemoteGxserverRequestTarget,
) -> Result<()> {
    browser.set_nodelay(true)?;
    let (host, port) =
        tokio::time::timeout(Duration::from_secs(5), socks_destination(&mut browser)).await??;
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
            .append_pair("host", &host)
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
            browser.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
            bail!("The remote browser destination could not be reached.");
        }
    };
    browser.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
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
