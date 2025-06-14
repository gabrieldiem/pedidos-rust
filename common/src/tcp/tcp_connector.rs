use crate::constants::DEFAULT_PR_HOST;
use crate::protocol::SocketMessage;
use crate::utils::logger::Logger;
use actix::{Actor, Context};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::{TcpSocket, TcpStream, UdpSocket};

pub struct TcpConnector {
    pub logger: Logger,
    pub source_port: u32,
    pub dest_ports: Vec<u32>,
}

impl Actor for TcpConnector {
    type Context = Context<Self>;
}

/// Iterates through a list of ports until it can connect with 1
impl TcpConnector {
    const MAX_WAITING_PERIOD_UDP_IN_SECONDS: u64 = 1;
    const MAX_CONNECTION_PASSES: u64 = 3;

    pub fn new(source_port: u32, dest_ports: Vec<u32>) -> TcpConnector {
        TcpConnector {
            logger: Logger::new(Some("[TCP-CONNECTOR]")),
            source_port,
            dest_ports,
        }
    }

    async fn finish_connection(&self, port: u32) -> Result<TcpStream, Box<dyn std::error::Error>> {
        let server_sockaddr = format!("{}:{}", DEFAULT_PR_HOST, port);
        let socket = TcpSocket::new_v4()?;
        let local_addr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), self.source_port as u16);
        socket.bind(local_addr)?;

        match socket.connect(server_sockaddr.parse()?).await {
            Ok(stream_connected) => {
                self.logger
                    .info(&format!("Using address {}", stream_connected.local_addr()?));
                self.logger
                    .info(&format!("Connected to server {}", server_sockaddr));
                Ok(stream_connected)
            }
            Err(e) => {
                self.logger
                    .warn(&format!("Failed to connect to {}: {}", server_sockaddr, e));
                Err(e.into())
            }
        }
    }

    async fn check_liveness(
        &self,
        port: u32,
        udp_socket: &UdpSocket,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match Self::try_connection(port, udp_socket, &self.logger.clone()).await {
            Ok(_port) => {
                self.logger
                    .debug(&format!("Liveness check for {port} passed"));
                Ok(())
            }
            Err(e) => {
                self.logger
                    .warn(&format!("Liveness check for {port} failed"));
                Err(e)
            }
        }
    }

    pub async fn connect(&self) -> Result<TcpStream, Box<dyn std::error::Error>> {
        let local_addr =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), self.source_port as u16);

        let socket = match UdpSocket::bind(local_addr).await {
            Ok(socket) => socket,
            Err(e) => {
                self.logger.error(&format!("Could not get UDP socket: {e}"));
                return Err(e.into());
            }
        };

        for _ in 0..Self::MAX_CONNECTION_PASSES {
            for port in self.dest_ports.clone() {
                let server_sockaddr = format!("{}:{}", DEFAULT_PR_HOST, port);
                self.logger
                    .debug(&format!("Trying to connect to {}", server_sockaddr));
                match Self::try_connection(port, &socket, &self.logger.clone()).await {
                    Ok(port) => {
                        self.logger
                            .debug(&format!("Found port to connect to: {}", port));

                        if (self.check_liveness(port, &socket).await).is_err() {
                            continue;
                        }
                        return self.finish_connection(port).await;
                    }
                    Err(e) => {
                        self.logger
                            .warn(&format!("Failed to connect to port {}: {}", port, e));
                    }
                }
            }

            self.logger.info("Trying to connect again");
        }

        Err("Could not connect to any server".into())
    }

    fn serialize_message(message: SocketMessage) -> Result<String, Box<dyn std::error::Error>> {
        let msg_to_send = serde_json::to_string(&message)?;
        Ok(msg_to_send)
    }

    fn deserialize_message(
        buf: &[u8],
        size: usize,
    ) -> Result<SocketMessage, Box<dyn std::error::Error>> {
        let received = String::from_utf8_lossy(&buf[..size]).to_string();
        let parsed: SocketMessage = serde_json::from_str(&received)?;
        Ok(parsed)
    }

    #[allow(unreachable_patterns)]
    async fn try_connection(
        port: u32,
        socket: &UdpSocket,
        logger: &Logger,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        let server_sockaddr = format!("{}:{}", DEFAULT_PR_HOST, port);
        let msg = Self::serialize_message(SocketMessage::IsConnectionReady)?;
        socket
            .send_to(msg.as_bytes(), server_sockaddr.clone())
            .await?;

        let mut buf = [0; 1024];
        let timeout_duration =
            tokio::time::Duration::from_secs(Self::MAX_WAITING_PERIOD_UDP_IN_SECONDS);

        match tokio::time::timeout(timeout_duration, socket.recv_from(&mut buf)).await {
            Ok(Ok((size, _src_address))) => {
                let received_msg = Self::deserialize_message(&buf, size)?;
                match received_msg {
                    SocketMessage::ConnectionAvailable => Ok(port),
                    SocketMessage::ConnectionNotAvailable(port_to_communicate) => {
                        logger.debug(&format!(
                            "Connection not available, but {port_to_communicate} is"
                        ));
                        Ok(port_to_communicate)
                    }
                    _ => {
                        logger.warn(&format!("Unrecognized message: {:?}", received_msg));
                        Err("Unrecognized message".into())
                    }
                }
            }
            Ok(Err(e)) => {
                logger.warn(&format!("Failed to receive data: {}", e));
                Err(e.into())
            }
            Err(e) => {
                logger.warn(&format!(
                    "Timeout of {}s for {}: connection unresponsive",
                    timeout_duration.as_secs(),
                    server_sockaddr
                ));
                Err(e.into())
            }
        }
    }
}
