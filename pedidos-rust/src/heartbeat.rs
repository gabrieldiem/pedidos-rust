use crate::connection_manager::{ConnectionManager, PeerId};
use crate::messages::{GetPeers, LivenessProbe, StartHeartbeat};
use crate::server_peer::ServerPeer;
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, ResponseActFuture, WrapFuture};
use actix_async_handler::async_handler;
use common::utils::logger::Logger;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::time::Duration;

#[derive(Message, Debug)]
#[rtype(result = "()")]
struct BeginLivenessCheck {}

#[derive(Message, Debug)]
#[rtype(result = "()")]
struct ExecuteLivenessCheck {
    peers: HashMap<PeerId, Addr<ServerPeer>>,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
struct FinishLivenessCheck {}

pub struct HeartbeatMonitor {
    logger: Logger,
    connection_manager: Addr<ConnectionManager>,
    beat_count: u64,
    udp_socket: Option<Arc<UdpSocket>>,
}

impl HeartbeatMonitor {
    const BASE_LOGGER_PREFIX: &'static str = "HEARTBEAT";
    pub const HEARTBEAT_DELAY_IN_SECS: u64 = 3;

    pub fn new(connection_manager: Addr<ConnectionManager>) -> HeartbeatMonitor {
        HeartbeatMonitor {
            logger: Logger::new(Some(&format!("[{}]", Self::BASE_LOGGER_PREFIX))),
            connection_manager,
            beat_count: 0,
            udp_socket: None,
        }
    }

    async fn check_aliveness(
        _peer_id: u32,
        peer_addr: &Addr<ServerPeer>,
        udp_socket: Arc<UdpSocket>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        peer_addr.send(LivenessProbe { udp_socket }).await?;

        Ok(true)
    }
}

impl Actor for HeartbeatMonitor {
    type Context = Context<Self>;
}

#[async_handler]
impl Handler<StartHeartbeat> for HeartbeatMonitor {
    type Result = ();

    async fn handle(&mut self, msg: StartHeartbeat, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.info("Starting HeartbeatMonitor");
        self.udp_socket = Some(msg.udp_socket);

        _ctx.notify_later(
            BeginLivenessCheck {},
            Duration::from_secs(Self::HEARTBEAT_DELAY_IN_SECS),
        );
    }
}

#[async_handler]
impl Handler<BeginLivenessCheck> for HeartbeatMonitor {
    type Result = ();

    async fn handle(&mut self, _msg: BeginLivenessCheck, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.update_prefix(&format!(
            "[{} B-{}]",
            Self::BASE_LOGGER_PREFIX,
            self.beat_count
        ));

        let res_fut = self.connection_manager.send(GetPeers {}).await;
        if let Ok(Ok(peers)) = res_fut {
            _ctx.address().do_send(ExecuteLivenessCheck { peers })
        };
    }
}

impl Handler<ExecuteLivenessCheck> for HeartbeatMonitor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: ExecuteLivenessCheck, _ctx: &mut Self::Context) -> Self::Result {
        let peers = msg.peers;
        let logger = self.logger.clone();
        let udp_socket = self.udp_socket.clone();
        let my_address = _ctx.address().clone();

        Box::pin(
            async move {
                if let Some(udp_socket) = udp_socket {
                    let number_of_peers = peers.len();
                    logger.debug(&format!("Number of peers: {}", peers.len()));
                    for (peer_id, peer_addr) in peers {
                        let res =
                            Self::check_aliveness(peer_id, &peer_addr, udp_socket.clone()).await;
                        match res {
                            Ok(true) => {
                                logger.info("ALIVE");
                            }
                            Ok(false) => logger.info("DEAD"),
                            Err(err) => logger.info("ERROR ALIVE"),
                        }
                    }

                    my_address.do_send(FinishLivenessCheck {});
                }
            }
            .into_actor(self),
        )
    }
}

#[async_handler]
impl Handler<FinishLivenessCheck> for HeartbeatMonitor {
    type Result = ();

    async fn handle(
        &mut self,
        _msg: FinishLivenessCheck,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.beat_count += 1;

        _ctx.notify_later(
            BeginLivenessCheck {},
            Duration::from_secs(Self::HEARTBEAT_DELAY_IN_SECS),
        );
    }
}
