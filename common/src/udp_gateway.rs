use crate::configuration::Configuration;
use crate::protocol::{ReconnectToNewPedidosRust, SocketMessage};
use crate::utils::logger::Logger;
use actix::dev::ToEnvelope;
use actix::{Actor, Addr, Handler, Message};
use std::sync::Arc;
use tokio::net::UdpSocket;

pub struct InfoForUdpGatewayData {
    pub port: u32,
    pub configuration: Configuration,
    pub udp_socket: Arc<UdpSocket>,
}

#[derive(Message, Debug)]
#[rtype(result = "InfoForUdpGatewayData")]
pub struct InfoForUdpGatewayRequest {}

pub struct UdpGateway {}

pub trait UdpClientActor: Actor + Handler<ReconnectToNewPedidosRust>
where
    Self::Context: ToEnvelope<Self, ReconnectToNewPedidosRust>,
{
}

impl<A> UdpClientActor for A
where
    A: Actor + Handler<ReconnectToNewPedidosRust>,
    A::Context: ToEnvelope<A, ReconnectToNewPedidosRust>,
{
}

#[allow(dead_code)]
impl UdpGateway {
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

    pub async fn run<A>(
        _port: u32,
        logger: Logger,
        client: Addr<A>,
        _configuration: Configuration,
        socket: Arc<UdpSocket>,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        A: Actor + Handler<ReconnectToNewPedidosRust>,
        A::Context: ToEnvelope<A, ReconnectToNewPedidosRust>,
    {
        let mut buf = [0; 2048];

        loop {
            match socket.recv_from(&mut buf).await {
                Ok((size, _addr)) => {
                    let received_msg = Self::deserialize_message(&buf, size)?;
                    match received_msg {
                        SocketMessage::ReconnectionMandate(new_leader_id, new_leader_port) => {
                            logger.debug(&format!("Received ReconnectionMandate with new leader ID: {new_leader_id} and port {new_leader_port}"));
                            client.do_send(ReconnectToNewPedidosRust {
                                new_id: new_leader_id,
                                new_port: new_leader_port,
                            });
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
