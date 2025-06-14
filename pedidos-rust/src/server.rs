use crate::client_connection::ClientConnection;
use crate::connection_manager::ConnectionManager;
use crate::heartbeat::HeartbeatMonitor;
use actix::{Actor, Addr, StreamHandler};
use common::configuration::Configuration;
use common::constants::DEFAULT_PR_HOST;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use tokio::io::{AsyncBufReadExt, BufReader, split};
use tokio::net::TcpListener;
use tokio_stream::wrappers::LinesStream;

#[allow(dead_code)]
pub struct Server {
    id: u32,
    logger: Logger,
    configuration: Configuration,
    hearbeat_monitor: Addr<HeartbeatMonitor>,
    connection_manager: Addr<ConnectionManager>,
    is_leader: bool,
}

impl Server {
    pub fn new(id: u32) -> Result<Server, Box<dyn std::error::Error>> {
        let logger_prefix = format!("[PEDIDOS-RUST-{}]", id);
        let logger = Logger::new(Some(&logger_prefix));

        let connection_manager = ConnectionManager::create(|_ctx| ConnectionManager::new());
        let configuration = Configuration::new()?;
        let hearbeat_monitor =
            HeartbeatMonitor::create(|_ctx| HeartbeatMonitor::new(connection_manager.clone()));

        let is_leader = id == 1;

        Ok(Server {
            id,
            logger,
            configuration,
            hearbeat_monitor,
            connection_manager,
            is_leader,
        })
    }

    async fn run(&self, port: u32) {
        let sockaddr_str = format!("{}:{}", DEFAULT_PR_HOST, port);
        let listener = TcpListener::bind(sockaddr_str.clone()).await.unwrap();

        self.logger.info(&format!(
            "Listening for connections on {}",
            sockaddr_str.clone()
        ));

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
    }

    pub async fn start(&self) {
        let port_pair = self
            .configuration
            .pedidos_rust
            .ports
            .iter()
            .find(|pair| pair.id == self.id);
        match port_pair {
            Some(port_pair) => {
                self.run(port_pair.port).await;
            }
            None => {
                self.logger.error(&format!(
                    "Could not find port in configuration for id: {}",
                    self.id
                ));
            }
        }
    }
}
