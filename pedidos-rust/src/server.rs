use crate::client_connection::ClientConnection;
use crate::connection_manager::ConnectionManager;
use crate::heartbeat::HeartbeatMonitor;
use actix::{Actor, Addr, StreamHandler};
use common::configuration::Configuration;
use common::constants::DEFAULT_PR_HOST;
use common::protocol::SocketMessage;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::io::{AsyncBufReadExt, BufReader, split};
use tokio::net::TcpListener;
use tokio::net::UdpSocket;
use tokio::spawn;
use tokio_stream::wrappers::LinesStream;

#[allow(dead_code)]
pub struct Server {
    id: u32,
    logger: Logger,
    configuration: Configuration,
    hearbeat_monitor: Addr<HeartbeatMonitor>,
    connection_manager: Addr<ConnectionManager>,
    is_leader: bool,
    port: u32,
}

impl Server {
    const INITIAL_LEADER_ID: u32 = 1;

    pub async fn new(id: u32, logger: Logger) -> Result<Server, Box<dyn std::error::Error>> {
        let connection_manager = ConnectionManager::create(|_ctx| ConnectionManager::new());
        let configuration = Configuration::new()?;
        let hearbeat_monitor =
            HeartbeatMonitor::create(|_ctx| HeartbeatMonitor::new(connection_manager.clone()));

        let port_pair = configuration
            .pedidos_rust
            .ports
            .iter()
            .find(|pair| pair.id == id);

        let my_port: u32 = match port_pair {
            Some(port_pair) => port_pair.port,
            None => {
                let msg = format!("Could not find port in configuration for id: {}", id);
                logger.error(&msg);
                return Err(msg.into());
            }
        };

        let is_leader = id == Self::INITIAL_LEADER_ID;

        Ok(Server {
            id,
            logger,
            configuration,
            hearbeat_monitor,
            connection_manager,
            is_leader,
            port: my_port,
        })
    }

    fn deserialize_message(
        buf: &[u8],
        size: usize,
    ) -> Result<SocketMessage, Box<dyn std::error::Error>> {
        let received = String::from_utf8_lossy(&buf[..size]).to_string();
        let parsed: SocketMessage = serde_json::from_str(&received)?;
        Ok(parsed)
    }

    fn serialize_message(message: SocketMessage) -> Result<String, Box<dyn std::error::Error>> {
        let msg_to_send = serde_json::to_string(&message)?;
        Ok(msg_to_send)
    }

    async fn send_connection_answer(
        logger: &Logger,
        socket: &UdpSocket,
        addr: SocketAddr,
        is_leader: bool,
        leader_port: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let msg: String = if is_leader {
            logger.debug("Instance is leader. Connection available");
            Self::serialize_message(SocketMessage::ConnectionAvailable)?
        } else {
            logger.debug(&format!(
                "Instance is not leader. Connection refused. Leader is: {leader_port}"
            ));
            Self::serialize_message(SocketMessage::ConnectionNotAvailable(leader_port))?
        };
        socket.send_to(msg.as_bytes(), addr).await?;
        Ok(())
    }

    async fn run_connection_gateway_loop(
        port: u32,
        logger: Logger,
        is_leader: bool,
        leader_port: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port as u16);

        let socket = match UdpSocket::bind(local_addr).await {
            Ok(socket) => socket,
            Err(e) => {
                logger.error(&format!("Could not get UDP socket: {e}"));
                return Err(e.into());
            }
        };

        logger.info(&format!(
            "Connection Gateway over UDP listening on {}",
            local_addr
        ));

        let mut buf = [0; 2048];

        loop {
            match socket.recv_from(&mut buf).await {
                Ok((size, addr)) => {
                    let received_msg = Self::deserialize_message(&buf, size)?;
                    match received_msg {
                        SocketMessage::IsConnectionReady => {
                            logger.debug("Received connection probe");
                            Self::send_connection_answer(
                                &logger,
                                &socket,
                                addr,
                                is_leader,
                                leader_port,
                            )
                            .await?;
                        }
                        _ => {
                            logger.warn(&format!("Unrecognized message: {:?}", received_msg));
                        }
                    }
                }
                Err(e) => {
                    logger.error(&format!("UDP receive error: {}", e));
                }
            }
        }
    }

    fn run_connection_gateway(&self) -> Result<(), Box<dyn std::error::Error>> {
        let logger = Logger::new(Some("[CONN-GATEWAY]"));
        let port_clone = self.port;
        let is_leader_clone = self.is_leader;
        let port_pair = self
            .configuration
            .pedidos_rust
            .ports
            .iter()
            .find(|pair| pair.id == Self::INITIAL_LEADER_ID);

        let leader_port: u32 = match port_pair {
            Some(port_pair) => port_pair.port,
            None => {
                let msg = format!(
                    "Could not find port in configuration for id: {}",
                    Self::INITIAL_LEADER_ID
                );
                self.logger.error(&msg);
                return Err(msg.into());
            }
        };

        spawn(async move {
            if let Err(e) =
                Self::run_connection_gateway_loop(port_clone, logger, is_leader_clone, leader_port)
                    .await
            {
                eprintln!("UDP loop error: {}", e);
            }
        });

        Ok(())
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let sockaddr_str = format!("{}:{}", DEFAULT_PR_HOST, self.port);
        let listener = TcpListener::bind(sockaddr_str.clone()).await.unwrap();

        self.logger.info(&format!(
            "Listening for connections on {}",
            sockaddr_str.clone()
        ));

        self.run_connection_gateway()?;

        while let Ok((stream, client_sockaddr)) = listener.accept().await {
            self.logger
                .info(&format!("Client connected: {client_sockaddr}"));

            ClientConnection::create(|ctx| {
                self.logger.debug("Created ClientConnection");
                let (read_half, write_half) = split(stream);

                ClientConnection::add_stream(
                    LinesStream::new(BufReader::new(read_half).lines()),
                    ctx,
                );
                let tcp_sender = TcpSender {
                    write_stream: Some(write_half),
                }
                .start();

                let port = client_sockaddr.port() as u32;
                ClientConnection {
                    is_leader: self.is_leader,
                    tcp_sender,
                    logger: Logger::new(Some(&format!("[PEDIDOS-RUST] [CONN:{}]", &port))),
                    id: port,
                    connection_manager: self.connection_manager.clone(),
                    peer_location: None,
                }
            });
        }

        Ok(())
    }
}
