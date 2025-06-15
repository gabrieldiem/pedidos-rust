use crate::client_connection::ClientConnection;
use crate::connection_gateway::ConnectionGateway;
use crate::connection_manager::ConnectionManager;
use crate::heartbeat::HeartbeatMonitor;
use crate::messages::RegisterPeerServer;
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
    ports_for_peers: Vec<u32>,
    ports_for_peers_in_use: HashMap<u32, bool>,
}

impl Server {
    const INITIAL_LEADER_ID: u32 = 1;

    pub async fn new(id: u32, logger: Logger) -> Result<Server, Box<dyn std::error::Error>> {
        let connection_manager = ConnectionManager::create(|_ctx| ConnectionManager::new());
        let configuration = Configuration::new()?;

        let port_pair = configuration
            .pedidos_rust
            .infos
            .iter()
            .find(|pair| pair.id == id);

        let (my_port, ports_for_peers) = match port_pair {
            Some(port_pair) => (port_pair.port, port_pair.clone().ports_for_peers),
            None => {
                let msg = format!("Could not find port in configuration for id: {}", id);
                logger.error(&msg);
                return Err(msg.into());
            }
        };

        let is_leader = id == Self::INITIAL_LEADER_ID;

        let port_pair = configuration
            .pedidos_rust
            .infos
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
        let hearbeat_monitor =
            HeartbeatMonitor::create(|_ctx| HeartbeatMonitor::new(connection_manager.clone()));

        let mut ports_for_peers_in_use: HashMap<u32, bool> = HashMap::new();
        for port in ports_for_peers.clone() {
            ports_for_peers_in_use.insert(port, false);
        }

        Ok(Server {
            id,
            logger,
            configuration,
            hearbeat_monitor,
            connection_manager,
            is_leader,
            leader_port: Arc::new(Mutex::new(leader_port)),
            port: my_port,
            ports_for_peers,
            ports_for_peers_in_use,
        })
    }

    fn choose_port_for_peer(&mut self) -> Result<u32, Box<dyn std::error::Error>> {
        for port in self.ports_for_peers.clone() {
            match self.ports_for_peers_in_use.get(&port) {
                Some(false) => {
                    self.ports_for_peers_in_use.insert(port, true);
                    return Ok(port);
                }
                Some(true) => continue,
                None => {
                    return Err(format!("No such port in list of ports for peer: {port}").into());
                }
            }
        }

        Err("No ports available for peer connection".into())
    }

    fn set_peer_port_as_unused(&mut self, port_for_peer: u32) {
        self.ports_for_peers_in_use.insert(port_for_peer, false);
    }

    async fn connect_server_peers(&mut self) -> Result<u64, Box<dyn std::error::Error>> {
        for port_pair in self.configuration.pedidos_rust.infos.clone() {
            let id = port_pair.id;
            let port = port_pair.port;
            if id == self.id {
                continue;
            }
            let is_leader = Self::INITIAL_LEADER_ID == id;

            let port_for_peer = self.choose_port_for_peer()?;
            let tcp_connector = TcpConnector::new(port_for_peer, vec![port]);
            let stream = match tcp_connector.connect().await {
                Ok(stream) => stream,
                Err(e) => {
                    self.logger
                        .warn(&format!("Failed to establish stream: {e}"));
                    self.set_peer_port_as_unused(port_for_peer);
                    continue;
                }
            };

            self.logger
                .info(&format!("Correctly connected to port {}", port));

            let peer = ServerPeer::create(|ctx| {
                let (read_half, write_half) = split(stream);

                ServerPeer::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);

                let tcp_sender = TcpSender {
                    write_stream: Some(write_half),
                }
                .start();

                self.logger.debug("Created ServerPeer");

                ServerPeer {
                    tcp_sender,
                    logger: Logger::new(Some(&format!("[PEER-{port}]"))),
                    port,
                    connection_manager: self.connection_manager.clone(),
                }
            });

            self.connection_manager.do_send(RegisterPeerServer {
                id,
                address: peer,
                is_leader,
            });
        }
        Ok(1)
    }

    fn run_connection_gateway(&self) -> Result<(), Box<dyn std::error::Error>> {
        let logger = Logger::new(Some("[CONN-GATEWAY]"));
        let port_clone = self.port;
        let is_leader_clone = self.is_leader;
        let leader_port_clone = self.leader_port.clone();
        let connection_manager = self.connection_manager.clone();
        let configuration = self.configuration.clone();

        spawn(async move {
            if let Err(e) = ConnectionGateway::run(
                port_clone,
                logger.clone(),
                is_leader_clone,
                leader_port_clone,
                connection_manager,
                configuration,
            )
            .await
            {
                logger.error(&format!("Connection gateway error during loop: {}", e));
            }
        });

        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let sockaddr_str = format!("{}:{}", DEFAULT_PR_HOST, self.port);
        let listener = TcpListener::bind(sockaddr_str.clone()).await.unwrap();

        self.logger.info(&format!(
            "Listening for connections on {}",
            sockaddr_str.clone()
        ));

        self.connect_server_peers().await?;

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
