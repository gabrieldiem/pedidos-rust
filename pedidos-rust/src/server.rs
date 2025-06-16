use crate::client_connection::ClientConnection;
use crate::connection_gateway::ConnectionGateway;
use crate::connection_manager::ConnectionManager;
use crate::messages::{RegisterPeerServer, StartHeartbeat};
use crate::server_peer::ServerPeer;
use actix::{Actor, Addr, StreamHandler};
use common::configuration::Configuration;
use common::constants::DEFAULT_PR_HOST;
use common::tcp::tcp_connector::TcpConnector;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::{LogLevel, Logger};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader, split};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::spawn;
use tokio_stream::wrappers::LinesStream;

#[allow(dead_code)]
pub struct Server {
    id: u32,
    logger: Logger,
    configuration: Configuration,
    connection_manager: Addr<ConnectionManager>,
    port: u32,
    ports_for_peers: Vec<u32>,
    ports_for_peers_in_use: HashMap<u32, bool>,
}

impl Server {
    const HEARTBEAT_LOG_LEVEL: LogLevel = LogLevel::Debug;

    pub async fn new(id: u32, logger: Logger) -> Result<Server, Box<dyn std::error::Error>> {
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

        let connection_manager = ConnectionManager::create(|_ctx| {
            ConnectionManager::new(id, my_port, configuration.clone())
        });

        let mut ports_for_peers_in_use: HashMap<u32, bool> = HashMap::new();
        for port in ports_for_peers.clone() {
            ports_for_peers_in_use.insert(port, false);
        }

        Ok(Server {
            id,
            logger,
            configuration,
            connection_manager,
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

    async fn connect_server_peers(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for port_pair in self.configuration.pedidos_rust.infos.clone() {
            let peer_id = port_pair.id;
            let peer_port = port_pair.port;
            if peer_id == self.id {
                continue;
            }
            let port_for_peer = self.choose_port_for_peer()?;
            let tcp_connector = TcpConnector::new(port_for_peer, vec![peer_port]);
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
                .info(&format!("Correctly connected to port {}", peer_port));

            let peer = ServerPeer::create(|ctx| {
                let (read_half, write_half) = split(stream);

                ServerPeer::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);

                let tcp_sender = TcpSender {
                    write_stream: Some(write_half),
                }
                .start();

                ServerPeer::new(
                    self.id,
                    peer_id,
                    tcp_sender,
                    self.port,
                    peer_port,
                    self.connection_manager.clone(),
                    Self::HEARTBEAT_LOG_LEVEL,
                )
            });

            self.connection_manager.do_send(RegisterPeerServer {
                id: peer_id,
                address: peer,
            });
        }
        Ok(())
    }

    async fn run_connection_gateway(
        port: u32,
        id: u32,
        connection_manager: Addr<ConnectionManager>,
        configuration: Configuration,
    ) -> Result<Arc<UdpSocket>, Box<dyn std::error::Error>> {
        let logger = Logger::new(Some("[CONN-GATEWAY]"));

        let local_addr: SocketAddr = match format!("{}:{}", DEFAULT_PR_HOST, port).parse() {
            Ok(addr) => addr,
            Err(e) => return Err(e.into()),
        };

        let socket = match UdpSocket::bind(local_addr).await {
            Ok(socket) => Arc::new(socket),
            Err(e) => {
                logger.error(&format!("Could not get UDP socket: {e}"));
                return Err(e.into());
            }
        };

        logger.info(&format!(
            "Connection Gateway over UDP listening on {}",
            local_addr
        ));

        let socket_ref = socket.clone();

        let mut heart_beat_logger = logger.clone();
        heart_beat_logger.set_level(Self::HEARTBEAT_LOG_LEVEL);

        spawn(async move {
            if let Err(e) = ConnectionGateway::run(
                port,
                id,
                logger.clone(),
                heart_beat_logger,
                connection_manager,
                configuration,
                socket_ref,
            )
            .await
            {
                logger.error(&format!("Connection gateway error during loop: {}", e));
            }
        });

        Ok(socket)
    }

    fn get_cannonical_peer_port(&self, peer_port_from_connection: u32) -> Option<u32> {
        let mut canonical_peer_port: Option<u32> = None;

        for info in self.configuration.pedidos_rust.infos.clone() {
            for port_for_peer in info.ports_for_peers {
                if port_for_peer == peer_port_from_connection {
                    canonical_peer_port = Some(info.port);
                }
            }
        }

        canonical_peer_port
    }

    fn create_server_peer(
        &self,
        peer_sockaddr: SocketAddr,
        stream: TcpStream,
        peer_id: u32,
        _udp_socket: Arc<UdpSocket>,
    ) {
        self.logger
            .info(&format!("Peer connected: {peer_sockaddr}"));

        let peer_port_from_connection = peer_sockaddr.port() as u32;
        let peer_port = match self.get_cannonical_peer_port(peer_port_from_connection) {
            Some(port) => port,
            None => {
                self.logger.error(&format!(
                    "No cannonical peer port for: {}",
                    peer_port_from_connection
                ));
                return;
            }
        };

        let server_peer = ServerPeer::create(|ctx| {
            let (read_half, write_half) = split(stream);

            ServerPeer::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);

            let tcp_sender = TcpSender {
                write_stream: Some(write_half),
            }
            .start();

            ServerPeer::new(
                self.id,
                peer_id,
                tcp_sender,
                self.port,
                peer_port,
                self.connection_manager.clone(),
                Self::HEARTBEAT_LOG_LEVEL,
            )
        });

        self.connection_manager.do_send(RegisterPeerServer {
            id: peer_id,
            address: server_peer.clone(),
        });
    }

    fn create_client_connection(&self, client_sockaddr: SocketAddr, stream: TcpStream) {
        self.logger
            .info(&format!("Client connected: {client_sockaddr}"));

        ClientConnection::create(|ctx| {
            self.logger.debug("Created ClientConnection");
            let (read_half, write_half) = split(stream);

            ClientConnection::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);
            let tcp_sender = TcpSender {
                write_stream: Some(write_half),
            }
            .start();

            let port = client_sockaddr.port() as u32;
            ClientConnection {
                tcp_sender,
                logger: Logger::new(Some(&format!("[PEDIDOS-RUST] [CONN:{}]", &port))),
                id: port,
                connection_manager: self.connection_manager.clone(),
                peer_location: None,
            }
        });
    }

    async fn start_heartbeats_of_peers(&self, udp_socket: Arc<UdpSocket>) {
        let _ = self
            .connection_manager
            .send(StartHeartbeat { udp_socket })
            .await;
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let sockaddr_str = format!("{}:{}", DEFAULT_PR_HOST, self.port);
        let listener = TcpListener::bind(sockaddr_str.clone()).await.unwrap();

        let (udp_socket_res, connect_server_peers_res) = tokio::join!(
            Self::run_connection_gateway(
                self.port,
                self.id,
                self.connection_manager.clone(),
                self.configuration.clone()
            ),
            self.connect_server_peers(),
        );

        connect_server_peers_res?;
        let udp_socket = udp_socket_res?;

        self.start_heartbeats_of_peers(udp_socket.clone()).await;

        self.logger.info(&format!(
            "Listening for connections on {}",
            sockaddr_str.clone()
        ));

        while let Ok((stream, connected_sockaddr)) = listener.accept().await {
            let (is_peer, peer_id) =
                ConnectionGateway::is_connection_a_peer(&connected_sockaddr, &self.configuration);

            if is_peer {
                self.create_server_peer(connected_sockaddr, stream, peer_id, udp_socket.clone());
            } else {
                self.create_client_connection(connected_sockaddr, stream);
            }

            self.start_heartbeats_of_peers(udp_socket.clone()).await;
        }

        Ok(())
    }
}
