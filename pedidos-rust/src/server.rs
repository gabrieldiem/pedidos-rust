use crate::client_connection::ClientConnection;
use crate::connection_gateway::ConnectionGateway;
use crate::connection_manager::ConnectionManager;
use crate::heartbeat::HeartbeatMonitor;
use crate::server_peer::ServerPeer;
use actix::{Actor, Addr, StreamHandler};
use common::configuration::Configuration;
use common::constants::DEFAULT_PR_HOST;
use common::tcp::tcp_connector::TcpConnector;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader, split};
use tokio::net::TcpListener;
use tokio::spawn;
use tokio::sync::Mutex;
use tokio_stream::wrappers::LinesStream;

#[allow(dead_code)]
pub struct Server {
    id: u32,
    logger: Logger,
    configuration: Configuration,
    hearbeat_monitor: Addr<HeartbeatMonitor>,
    connection_manager: Addr<ConnectionManager>,
    is_leader: bool,
    leader_port: Arc<Mutex<u32>>,
    port: u32,
}

impl Server {
    const INITIAL_LEADER_ID: u32 = 1;

    pub async fn new(id: u32, logger: Logger) -> Result<Server, Box<dyn std::error::Error>> {
        let connection_manager = ConnectionManager::create(|_ctx| ConnectionManager::new());
        let configuration = Configuration::new()?;

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

        let port_pair = configuration
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
                logger.error(&msg);
                return Err(msg.into());
            }
        };
        let server_peers = Self::connect_server_peers(
            id,
            my_port,
            configuration.clone(),
            &logger,
            connection_manager.clone(),
        )
        .await?;
        let hearbeat_monitor =
            HeartbeatMonitor::create(|_ctx| HeartbeatMonitor::new(connection_manager.clone()));

        Ok(Server {
            id,
            logger,
            configuration,
            hearbeat_monitor,
            connection_manager,
            is_leader,
            leader_port: Arc::new(Mutex::new(leader_port)),
            port: my_port,
        })
    }

    async fn connect_server_peers(
        id: u32,
        my_port: u32,
        configuration: Configuration,
        logger: &Logger,
        connection_manager: Addr<ConnectionManager>,
    ) -> Result<HashMap<u32, Addr<ServerPeer>>, Box<dyn std::error::Error>> {
        let mut server_peers: HashMap<u32, Addr<ServerPeer>> = HashMap::new();

        for port_pair in configuration.pedidos_rust.ports {
            let port = port_pair.port;
            if port == id {
                continue;
            }

            let tcp_connector = TcpConnector::new(my_port, vec![port]);
            let stream = match tcp_connector.connect().await {
                Ok(stream) => stream,
                Err(e) => {
                    continue;
                }
            };

            let peer = ServerPeer::create(|ctx| {
                let (read_half, write_half) = split(stream);

                ServerPeer::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);

                let tcp_sender = TcpSender {
                    write_stream: Some(write_half),
                }
                .start();

                logger.debug("Created ServerPeer");

                ServerPeer {
                    tcp_sender,
                    logger: Logger::new(Some(&format!("[PEER-{port}]"))),
                    port,
                    connection_manager: connection_manager.clone(),
                }
            });

            server_peers.insert(port, peer);
        }

        Ok(server_peers)
    }

    fn run_connection_gateway(&self) -> Result<(), Box<dyn std::error::Error>> {
        let logger = Logger::new(Some("[CONN-GATEWAY]"));
        let port_clone = self.port;
        let is_leader_clone = self.is_leader;
        let leader_port_clone = self.leader_port.clone();

        spawn(async move {
            if let Err(e) = ConnectionGateway::run(
                port_clone,
                logger.clone(),
                is_leader_clone,
                leader_port_clone,
            )
            .await
            {
                logger.error(&format!("Connection gateway error during loop: {}", e));
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
