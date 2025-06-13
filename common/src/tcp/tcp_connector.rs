use crate::constants::DEFAULT_PR_HOST;
use crate::utils::logger::Logger;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::{TcpSocket, TcpStream};

pub struct TcpConnector {
    logger: Logger,
    source_port: u32,
    dest_ports: Vec<u32>,
}

/// Iterates through a list of ports until it can connect with 1
impl TcpConnector {
    pub fn new(source_port: u32, dest_ports: Vec<u32>) -> TcpConnector {
        TcpConnector {
            logger: Logger::new(Some("TCP-CONNECTOR")),
            source_port,
            dest_ports,
        }
    }

    pub async fn connect(&self) -> Result<TcpStream, Box<dyn std::error::Error>> {
        for port in self.dest_ports.clone() {
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
                    return Ok(stream_connected);
                }
                Err(e) => {
                    self.logger
                        .warn(&format!("Failed to connect to {}: {}", server_sockaddr, e));
                }
            }
        }

        Err("Could not connect to any server".into())
    }
}
