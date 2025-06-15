use crate::connection_manager::ConnectionManager;
use crate::messages::Start;
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message};
use actix_async_handler::async_handler;
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
    pub const HEARTBEAT_DELAY_IN_SECS: u64 = 3;

    pub fn new(connection_manager: Addr<ConnectionManager>) -> HeartbeatMonitor {
        HeartbeatMonitor {
            logger: Logger::new(Some("[HEARTBEAT]")),
            connection_manager,
            beat_count: 0,
        }
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
        self.logger.debug(&format!("Beat {}", self.beat_count));

        self.beat_count += 1;
        _ctx.notify_later(
            CheckLiveness {},
            Duration::from_secs(Self::HEARTBEAT_DELAY_IN_SECS),
        );
    }
}
