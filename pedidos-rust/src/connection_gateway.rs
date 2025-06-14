use common::protocol::SocketMessage;
use common::utils::logger::Logger;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

pub struct ConnectionGateway {}

impl ConnectionGateway {
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
        leader_port: Arc<Mutex<u32>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        
        
        
        let msg: String = if is_leader {
            logger.debug("Instance is leader. Connection available");
            Self::serialize_message(SocketMessage::ConnectionAvailable)?
        } else {
            let leader_port = leader_port.lock().await;
            logger.debug(&format!(
                "Instance is not leader. Connection refused. Leader is: {leader_port}"
            ));
            Self::serialize_message(SocketMessage::ConnectionNotAvailable(*leader_port))?
        };
        socket.send_to(msg.as_bytes(), addr).await?;
        Ok(())
    }

    pub async fn run(
        port: u32,
        logger: Logger,
        is_leader: bool,
        leader_port: Arc<Mutex<u32>>,
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
                                leader_port.clone(),
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
