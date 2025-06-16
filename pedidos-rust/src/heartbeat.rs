use crate::connection_manager::ConnectionManager;
use crate::messages::{GetPeers, Start};
use crate::server_peer::ServerPeer;
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message};
use actix_async_handler::async_handler;
use common::protocol::LivenessProbe;
use common::utils::logger::Logger;
use tokio::time::Duration;

#[derive(Message, Debug)]
#[rtype(result = "()")]
struct CheckLiveness {}

pub struct HeartbeatMonitor {
    logger: Logger,
    connection_manager: Addr<ConnectionManager>,
    beat_count: u64,
}

impl HeartbeatMonitor {
    const BASE_LOGGER_PREFIX: &'static str = "HEARTBEAT";
    pub const HEARTBEAT_DELAY_IN_SECS: u64 = 3;

    pub fn new(connection_manager: Addr<ConnectionManager>) -> HeartbeatMonitor {
        HeartbeatMonitor {
            logger: Logger::new(Some(&format!("[{}]", Self::BASE_LOGGER_PREFIX))),
            connection_manager,
            beat_count: 0,
        }
    }

    async fn check_aliveness(
        &self,
        peer_id: u32,
        peer_addr: &Addr<ServerPeer>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        peer_addr.send(LivenessProbe {}).await?;

        Ok(true)
    }
}

impl Actor for HeartbeatMonitor {
    type Context = Context<Self>;
}

#[async_handler]
impl Handler<Start> for HeartbeatMonitor {
    type Result = ();

    async fn handle(&mut self, _msg: Start, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.info("Starting HeartbeatMonitor");

        _ctx.notify_later(
            CheckLiveness {},
            Duration::from_secs(Self::HEARTBEAT_DELAY_IN_SECS),
        );
    }
}

#[async_handler]
impl Handler<CheckLiveness> for HeartbeatMonitor {
    type Result = ();

    async fn handle(&mut self, _msg: CheckLiveness, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.update_prefix(&format!(
            "[{} B-{}]",
            Self::BASE_LOGGER_PREFIX,
            self.beat_count
        ));

        let fut = self.connection_manager.send(GetPeers {});
        let res_fut = fut.await;

        if let Ok(res_peers) = res_fut {
            if let Ok(peers) = res_peers {
                for (peer_id, peer_addr) in peers {
                    //self.check_aliveness(peer_id, &peer_addr).await?;
                }
            }
        }

        self.beat_count += 1;
        _ctx.notify_later(
            CheckLiveness {},
            Duration::from_secs(Self::HEARTBEAT_DELAY_IN_SECS),
        );
    }
}
