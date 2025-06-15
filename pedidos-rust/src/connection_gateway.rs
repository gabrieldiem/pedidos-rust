use crate::connection_manager::{ConnectionManager, LeaderData};
use crate::messages::{GetLeaderInfo, IsPeerConnected};
use actix::Addr;
use common::configuration::Configuration;
use common::protocol::{SocketMessage, UNKNOWN_LEADER};
use common::utils::logger::Logger;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::UdpSocket;

pub struct ConnectionGateway {}

impl ConnectionGateway {
    const NOT_A_PEER_PORT: u32 = 0;

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

    pub fn is_connection_a_peer(addr: &SocketAddr, configuration: &Configuration) -> (bool, u32) {
        let port_used = addr.port() as u32;

        for port_pair in configuration.pedidos_rust.infos.clone() {
            for a_port in port_pair.ports_for_peers {
                if a_port == port_used {
                    return (true, port_pair.id);
                }
            }
        }

        (false, Self::NOT_A_PEER_PORT)
    }

    async fn handle_incomming_peer_request(
        connection_manager: Addr<ConnectionManager>,
        peer_id: u32,
        leader_port: u32,
        logger: &Logger,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let msg = if let Ok(is_peer_connected) = connection_manager
            .send(IsPeerConnected { id: peer_id })
            .await?
        {
            if is_peer_connected {
                logger.debug(&format!(
                    "Peer with ID {peer_id} already connected, refusing connection"
                ));
                Self::serialize_message(SocketMessage::ConnectionNotAvailable(leader_port))?
            } else {
                logger.debug(&format!(
                    "Peer with ID {peer_id} not connected, accepting connection"
                ));
                Self::serialize_message(SocketMessage::ConnectionAvailableForPeer)?
            }
        } else {
            Self::serialize_message(SocketMessage::ConnectionNotAvailable(leader_port))?
        };

        Ok(msg)
    }

    async fn get_leader_information(
        connection_manager: &Addr<ConnectionManager>,
    ) -> Result<Option<LeaderData>, Box<dyn std::error::Error>> {
        if let Ok(leader) = connection_manager.send(GetLeaderInfo {}).await? {
            Ok(leader)
        } else {
            Ok(None)
        }
    }

    async fn send_connection_answer(
        logger: &Logger,
        socket: &UdpSocket,
        addr: SocketAddr,
        connection_manager: Addr<ConnectionManager>,
        configuration: &Configuration,
        id: u32,
        _port: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let leader_data = Self::get_leader_information(&connection_manager).await?;

        let (is_current_server_leader, leader_port) = match leader_data {
            Some(leader_data) => (leader_data.id == id, leader_data.port),
            None => (false, UNKNOWN_LEADER),
        };

        let (is_connection_a_peer, peer_id) = Self::is_connection_a_peer(&addr, configuration);

        let msg: String = if is_connection_a_peer {
            Self::handle_incomming_peer_request(connection_manager, peer_id, leader_port, logger)
                .await?
        } else if is_current_server_leader {
            logger.debug("Instance is leader. Connection available");
            Self::serialize_message(SocketMessage::ConnectionAvailable)?
        } else {
            let leader_string = if leader_port == UNKNOWN_LEADER {
                "unknown"
            } else {
                &leader_port.to_string()
            };
            logger.debug(&format!(
                "Instance is not leader. Connection refused. Leader is: {leader_string}"
            ));
            Self::serialize_message(SocketMessage::ConnectionNotAvailable(leader_port))?
        };

        socket.send_to(msg.as_bytes(), addr).await?;

        Ok(())
    }

    pub async fn run(
        port: u32,
        id: u32,
        logger: Logger,
        connection_manager: Addr<ConnectionManager>,
        configuration: Configuration,
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
                                connection_manager.clone(),
                                &configuration,
                                id,
                                port,
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
}
