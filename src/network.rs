// src/network.rs
//! Distributed Networking, Remote Resolution, and Heartbeat Monitoring

use crate::mailbox::Message;
use crate::pid::Pid;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{interval, Duration};

pub struct NetworkManager {
    runtime: Arc<crate::Runtime>,
}

impl NetworkManager {
    pub fn new(runtime: Arc<crate::Runtime>) -> Self {
        Self { runtime }
    }

    /// Starts the TCP server for inter-node communication.
    pub async fn start_server(&self, addr: &str) -> std::io::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        let rt = self.runtime.clone();

        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let rt_inner = rt.clone();
                tokio::spawn(async move {
                    let mut head = [0u8; 1];
                    while socket.read_exact(&mut head).await.is_ok() {
                        match head[0] {
                            0 => {
                                // User Message: [PID:u64][LEN:u32][DATA]
                                let mut meta = [0u8; 12];
                                if socket.read_exact(&mut meta).await.is_err() {
                                    break;
                                }
                                let mut cursor = std::io::Cursor::new(&meta);
                                let pid = cursor.get_u64();
                                let len = cursor.get_u32() as usize;
                                let mut data = vec![0u8; len];
                                if socket.read_exact(&mut data).await.is_err() {
                                    break;
                                }
                                let _ = rt_inner.send(pid, Message::User(Bytes::from(data)));
                            }
                            1 => {
                                // Resolve Request: [LEN:u32][NAME:String] -> [PID:u64]
                                let mut len_buf = [0u8; 4];
                                if socket.read_exact(&mut len_buf).await.is_err() {
                                    break;
                                }
                                let len = u32::from_be_bytes(len_buf) as usize;
                                let mut name_vec = vec![0u8; len];
                                if socket.read_exact(&mut name_vec).await.is_err() {
                                    break;
                                }
                                let name = String::from_utf8_lossy(&name_vec);

                                let pid = rt_inner.resolve(&name).unwrap_or(0);
                                if socket.write_all(&pid.to_be_bytes()).await.is_err() {
                                    break;
                                }
                            }
                            2 => {
                                // Heartbeat (Ping) -> Returns 0x03 (Pong)
                                if socket.write_all(&[3u8]).await.is_err() {
                                    break;
                                }
                            }
                            _ => break,
                        }
                    }
                });
            }
        });
        Ok(())
    }

    /// Queries a remote node for a PID associated with a name.
    pub async fn resolve_remote(&self, addr: &str, name: &str) -> std::io::Result<Pid> {
        let mut stream = TcpStream::connect(addr).await?;
        stream.write_all(&[1u8]).await?; // Type 1: Resolve
        let name_bytes = name.as_bytes();
        stream
            .write_all(&(name_bytes.len() as u32).to_be_bytes())
            .await?;
        stream.write_all(name_bytes).await?;

        let mut pid_buf = [0u8; 8];
        stream.read_exact(&mut pid_buf).await?;
        Ok(u64::from_be_bytes(pid_buf))
    }

    /// Transmits a binary payload to a remote PID.
    pub async fn send_remote(&self, addr: &str, pid: Pid, data: Bytes) -> std::io::Result<()> {
        let mut stream = TcpStream::connect(addr).await?;
        stream.write_all(&[0u8]).await?; // Type 0: Send
        let mut buf = BytesMut::with_capacity(12 + data.len());
        buf.put_u64(pid);
        buf.put_u32(data.len() as u32);
        buf.put(data);
        stream.write_all(&buf).await?;
        Ok(())
    }

    /// Periodically probes a remote node to ensure it is alive.
    /// If the probe fails, it notifies the supervisor to trigger failover logic.
    pub async fn monitor_remote(&self, addr: String, pid: Pid, interval_ms: u64) {
        let rt = self.runtime.clone();
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_millis(interval_ms));
            loop {
                tick.tick().await;
                let is_up = match TcpStream::connect(&addr).await {
                    Ok(mut stream) => {
                        if stream.write_all(&[2u8]).await.is_ok() {
                            let mut pong = [0u8; 1];
                            stream.read_exact(&mut pong).await.is_ok() && pong[0] == 3
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                };

                if !is_up {
                    rt.supervisor().notify_exit(pid);
                    break;
                }
            }
        });
    }
}
