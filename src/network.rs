// src/network.rs
//! Distributed Networking, Remote Resolution, and Heartbeat Monitoring

use crate::mailbox::Message;
use crate::pid::Pid;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Duration};
use tracing::{debug, info, warn};


/// Current binary protocol version.  Peers must match or the connection is
/// immediately dropped.  Future releases will bump this when the wire format
/// changes; nodes on mixed-version clusters should handle rejections gracefully.
const PROTOCOL_VERSION: u8 = 1;

/// How large a single user payload can be before the connection is torn down.
const MAX_PAYLOAD_LEN: usize = 1024 * 1024; // 1 MiB

/// How many bytes a service name may contain when doing a remote resolve.
const MAX_NAME_LEN: usize = 1024;

/// Default I/O timeout if the runtime has not overridden it.
const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Types of messages that can appear on the wire.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum MessageType {
    User = 0,
    Resolve = 1,
    Ping = 2,
}

impl TryFrom<u8> for MessageType {
    type Error = ();
    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(MessageType::User),
            1 => Ok(MessageType::Resolve),
            2 => Ok(MessageType::Ping),
            _ => Err(()),
        }
    }
}

pub struct NetworkManager {
    runtime: Arc<crate::Runtime>,
}

impl NetworkManager {
    pub fn new(runtime: Arc<crate::Runtime>) -> Self {
        Self { runtime }
    }

    /// Starts the TCP server for inter-node communication.
    ///
    /// The return value is the actual socket address the listener bound to.
    /// This is useful when the caller passes `"127.0.0.1:0"` and needs to know
    /// which port the operating system selected.
    pub async fn start_server(&self, addr: &str) -> std::io::Result<std::net::SocketAddr> {
        let listener = TcpListener::bind(addr).await?;
        let actual_addr = listener.local_addr()?;
        info!(%actual_addr, "network server listening");
        let rt = self.runtime.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((socket, peer)) => {
                        debug!(%peer, "accepted connection");
                        let rt_inner = rt.clone();
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_connection(socket, rt_inner).await {
                                debug!(error = %e, "connection handler terminated");
                            }
                        });
                    }
                    Err(e) => {
                        warn!(error = %e, "error accepting connection, retrying");
                        continue;
                    }
                }
            }
        });

        Ok(actual_addr)
    }

/// Helper that wraps a read operation with a timeout and converts timeout
/// into an `std::io::Error` so callers can use the standard `?` operator.
async fn read_exact_with_timeout(
    socket: &mut TcpStream,
    buf: &mut [u8],
    dur: Duration,
) -> std::io::Result<()> {
    // like AsyncReadExt::read_exact but with an adjustable timeout value.
    let mut offset = 0;
    while offset < buf.len() {
        let n = match timeout(dur, socket.read(&mut buf[offset..])).await {
            Ok(Ok(0)) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "connection closed",
                ));
            }
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "read timed out",
                ))
            }
        };
        offset += n;
    }
    Ok(())
}

async fn write_all_with_timeout(socket: &mut TcpStream, buf: &[u8], dur: Duration) -> std::io::Result<()> {
    match timeout(dur, socket.write_all(buf)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "write timed out",
        )),
    }
}

/// Core connection loop extracted for clarity and testability.
async fn handle_connection(
    mut socket: TcpStream,
    rt: Arc<crate::Runtime>,
) -> std::io::Result<()> {
    // negotiate protocol version
    let io_timeout = rt.get_network_io_timeout();
    let mut version = [0u8; 1];
    if Self::read_exact_with_timeout(&mut socket, &mut version, io_timeout).await.is_err() {
        warn!("handshake read failed, closing");
        return Ok(());
    }
    if version[0] != PROTOCOL_VERSION {
        warn!(got = version[0], expected = PROTOCOL_VERSION, "protocol version mismatch");
        return Ok(());
    }

    let mut header = [0u8; 1];
    while Self::read_exact_with_timeout(&mut socket, &mut header, io_timeout).await.is_ok() {
        let msg_type = match MessageType::try_from(header[0]) {
            Ok(mt) => mt,
            Err(_) => {
                warn!(byte = header[0], "unknown message type, closing");
                break;
            }
        };

        match msg_type {
            MessageType::User => {
                // [PID:u64][LEN:u32][DATA]
                let mut meta = [0u8; 12];
                if Self::read_exact_with_timeout(&mut socket, &mut meta, io_timeout).await.is_err() {
                    break;
                }
                let mut cursor = std::io::Cursor::new(&meta);
                let pid = cursor.get_u64();
                let len = cursor.get_u32() as usize;
                if len > rt.get_network_max_payload() {
                    warn!(pid, len, "payload too large, dropping connection");
                    break;
                }
                let mut data = vec![0u8; len];
                if Self::read_exact_with_timeout(&mut socket, &mut data, io_timeout).await.is_err() {
                    break;
                }
                let _ = rt.send(pid, Message::User(Bytes::from(data)));
            }
            MessageType::Resolve => {
                // [LEN:u32][NAME:String] -> [PID:u64]
                let mut len_buf = [0u8; 4];
                if Self::read_exact_with_timeout(&mut socket, &mut len_buf, io_timeout).await.is_err() {
                    break;
                }
                let len = u32::from_be_bytes(len_buf) as usize;
                if len > rt.get_network_max_name_len() {
                    warn!(len, "resolve name too long, closing");
                    break;
                }
                let mut name_vec = vec![0u8; len];
                if Self::read_exact_with_timeout(&mut socket, &mut name_vec, io_timeout).await.is_err() {
                    break;
                }
                let name = match String::from_utf8(name_vec) {
                    Ok(s) => s,
                    Err(_) => {
                        warn!("invalid utf8 in resolve request");
                        break;
                    }
                };

                let pid = rt.resolve(&name).unwrap_or(0);
                if Self::write_all_with_timeout(&mut socket, &pid.to_be_bytes(), io_timeout).await.is_err() {
                    break;
                }
            }
            MessageType::Ping => {
                // Heartbeat (Ping) -> Returns 0x03 (Pong)
                if Self::write_all_with_timeout(&mut socket, &[3u8], io_timeout).await.is_err() {
                    break;
                }
            }
        }
    }
    Ok(())
}

    /// Queries a remote node for a PID associated with a name.
    pub async fn resolve_remote(&self, addr: &str, name: &str) -> std::io::Result<Pid> {
        debug!(%addr, %name, "resolving remote name");
          let io_timeout = self.runtime.get_network_io_timeout();
          let mut stream = timeout(io_timeout, TcpStream::connect(addr)).await??;
        // handshake
          Self::write_all_with_timeout(&mut stream, &[PROTOCOL_VERSION], io_timeout).await?;
        // Type 1: Resolve
          Self::write_all_with_timeout(&mut stream, &[MessageType::Resolve as u8], io_timeout).await?;
        let name_bytes = name.as_bytes();
        if name_bytes.len() > self.runtime.get_network_max_name_len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "name too long",
            ));
        }
        Self::write_all_with_timeout(&mut stream, &(name_bytes.len() as u32).to_be_bytes(), io_timeout).await?;
        Self::write_all_with_timeout(&mut stream, name_bytes, io_timeout).await?;

        let mut pid_buf = [0u8; 8];
        Self::read_exact_with_timeout(&mut stream, &mut pid_buf, io_timeout).await?;
        Ok(u64::from_be_bytes(pid_buf))
    }

    /// Transmits a binary payload to a remote PID.
    pub async fn send_remote(&self, addr: &str, pid: Pid, data: Bytes) -> std::io::Result<()> {
        debug!(%addr, pid, "sending remote message");
        if data.len() > self.runtime.get_network_max_payload() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "payload too large",
            ));
        }
          let io_timeout = self.runtime.get_network_io_timeout();
          let mut stream = timeout(io_timeout, TcpStream::connect(addr)).await??;
        // handshake
          Self::write_all_with_timeout(&mut stream, &[PROTOCOL_VERSION], io_timeout).await?;
          Self::write_all_with_timeout(&mut stream, &[MessageType::User as u8], io_timeout).await?;
        let mut buf = BytesMut::with_capacity(12 + data.len());
        buf.put_u64(pid);
        buf.put_u32(data.len() as u32);
        buf.put(data);
        Self::write_all_with_timeout(&mut stream, &buf, io_timeout).await?;
        Ok(())
    }

    /// Periodically probes a remote node to ensure it is alive.
    /// If the probe fails, it notifies the supervisor to trigger failover logic.
    pub async fn monitor_remote(&self, addr: String, pid: Pid, interval_ms: u64) {
        let rt = self.runtime.clone();
        tokio::spawn(async move {
            // use the base interval provided by the caller; backoff will
            // adjust it on failure.
            let mut curr = Duration::from_millis(interval_ms);
            let base = curr;
            let factor = rt.get_monitor_backoff_factor();
            let max_backoff = rt.get_monitor_backoff_max();
            let threshold = rt.get_monitor_failure_threshold();
            let mut failures = 0;
            loop {
                tokio::time::sleep(curr).await;
                let is_up = match timeout(rt.get_network_io_timeout(), TcpStream::connect(&addr)).await {
                    Ok(Ok(mut stream)) => {
                        // handshake with version before ping
                        if Self::write_all_with_timeout(&mut stream, &[PROTOCOL_VERSION], rt.get_network_io_timeout()).await.is_ok() &&
                           Self::write_all_with_timeout(&mut stream, &[MessageType::Ping as u8], rt.get_network_io_timeout()).await.is_ok() {
                            let mut pong = [0u8; 1];
                            Self::read_exact_with_timeout(&mut stream, &mut pong, rt.get_network_io_timeout()).await.is_ok() && pong[0] == 3
                        } else {
                            false
                        }
                    }
                    _ => false,
                };

                if !is_up {
                    failures += 1;
                    if failures >= threshold {
                        warn!(%addr, pid, "remote monitor detected node down after {} failures", failures);
                        rt.stop(pid);
                        rt.supervisor().notify_exit(pid);
                        break;
                    }
                    // exponential backoff before next attempt
                    let next = Duration::from_secs_f64(curr.as_secs_f64() * factor);
                    curr = if next > max_backoff { max_backoff } else { next };
                } else {
                    // reset on success
                    failures = 0;
                    curr = base;
                }
            }
        });
    }
}

// ------ tests ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Runtime;
    use bytes::BytesMut;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::net::TcpStream;
    use tokio::time::Duration;

    async fn setup_runtime_and_server() -> (Arc<Runtime>, std::net::SocketAddr) {
        let rt = Arc::new(Runtime::new());
        let manager = NetworkManager::new(rt.clone());
        let addr = manager.start_server("127.0.0.1:0").await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        (rt, addr)
    }

    #[tokio::test]
    async fn oversized_payload_is_dropped_but_server_remains() {
        let (rt, addr) = setup_runtime_and_server().await;

        let counter = Arc::new(AtomicUsize::new(0));
        let c_clone = counter.clone();
        let pid = rt.spawn_handler_with_budget(
            move |msg| {
                let c_clone = c_clone.clone();
                async move {
                    if let Message::User(_buf) = msg {
                        c_clone.fetch_add(1, Ordering::SeqCst);
                    }
                }
            },
            10,
        );

        // send a valid message
        let mut stream = TcpStream::connect(&addr).await.unwrap();
        stream.write_all(&[PROTOCOL_VERSION]).await.unwrap();
        let data = b"hello";
        let mut buf = BytesMut::with_capacity(13 + data.len());
        buf.put_u8(MessageType::User as u8);
        buf.put_u64(pid);
        buf.put_u32(data.len() as u32);
        buf.put_slice(data);
        stream.write_all(&buf).await.unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(rt.is_alive(pid));

        // send oversized message header only
        let mut stream2 = TcpStream::connect(&addr).await.unwrap();
        stream2.write_all(&[PROTOCOL_VERSION]).await.unwrap();
        let too_big = (MAX_PAYLOAD_LEN + 1) as u32;
        stream2.write_all(&[MessageType::User as u8]).await.unwrap();
        stream2.write_all(&pid.to_be_bytes()).await.unwrap();
        stream2.write_all(&too_big.to_be_bytes()).await.unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // server still handles a later good message
        let mut stream3 = TcpStream::connect(&addr).await.unwrap();
        stream3.write_all(&[PROTOCOL_VERSION]).await.unwrap();
        let data2 = b"again";
        let mut buf2 = BytesMut::with_capacity(13 + data2.len());
        buf2.put_u8(MessageType::User as u8);
        buf2.put_u64(pid);
        buf2.put_u32(data2.len() as u32);
        buf2.put_slice(data2);
        stream3.write_all(&buf2).await.unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn protocol_version_mismatch_closes_connection() {
        let (rt, addr) = setup_runtime_and_server().await;

        // actor that would otherwise receive a message
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_for_spawn = counter.clone();
        let pid = rt.spawn_handler_with_budget(
            move |msg| {
                let c = counter_for_spawn.clone();
                async move {
                    if let Message::User(_buf) = msg {
                        c.fetch_add(1, Ordering::SeqCst);
                    }
                }
            },
            1,
        );

        // craft a connection that sends wrong version
        let mut stream = TcpStream::connect(&addr).await.unwrap();
        stream.write_all(&[PROTOCOL_VERSION + 1]).await.unwrap();
        // then attempt normal user frame
        let data = b"will not be seen";
        let mut buf = BytesMut::with_capacity(13 + data.len());
        buf.put_u8(MessageType::User as u8);
        buf.put_u64(pid);
        buf.put_u32(data.len() as u32);
        buf.put_slice(data);
        let _ = stream.write_all(&buf).await;

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn configurable_limits_and_timeout() {
        let (rt, addr) = setup_runtime_and_server().await;
        rt.set_network_max_payload(5);
        rt.set_network_max_name_len(4);
        rt.set_network_io_timeout(Duration::from_millis(1));

        // payload limit enforced via the manager API
        let manager = NetworkManager::new(rt.clone());
        let err = manager.send_remote(&addr.to_string(), 0, Bytes::from_static(b"longer")).await.err();
        assert!(err.is_some());

        // name length limit
        let res = manager.resolve_remote(&addr.to_string(), "toolong").await;
        assert!(res.is_err());

        // timeout observed: connect to a server that accepts but never responds
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bad_addr = listener.local_addr().unwrap();
        // spawn accept but never read
        tokio::spawn(async move {
            if let Ok((_stream, _)) = listener.accept().await {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        let manager = NetworkManager::new(rt.clone());
        let e = manager.resolve_remote(&bad_addr.to_string(), "a").await.unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::TimedOut);
    }

    #[tokio::test]
    async fn monitor_backoff_threshold_behavior() {
        let rt = Arc::new(Runtime::new());
        rt.set_monitor_backoff(2.0, Duration::from_millis(50), 2);

        let pid = rt.spawn_actor(|mut rx| async move { let _ = rx.recv().await; });
        // monitor an address that doesn't exist
        let addr = "127.0.0.1:59999".to_string();
        let manager = NetworkManager::new(rt.clone());
        manager.monitor_remote(addr, pid, 10).await;

        // after one interval actor should still be alive
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(rt.is_alive(pid));

        // after enough time for two failures, actor should be gone
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(!rt.is_alive(pid));
    }
}