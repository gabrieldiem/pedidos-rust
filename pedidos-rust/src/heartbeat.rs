use crate::connection_manager::ConnectionManager;
use actix::{Actor, Addr, Context};
use common::utils::logger::Logger;

#[allow(dead_code)]
pub struct HeartbeatMonitor {
    logger: Logger,
    connection_manager: Addr<ConnectionManager>,
}

impl HeartbeatMonitor {
    pub fn new(connection_manager: Addr<ConnectionManager>) -> HeartbeatMonitor {
        HeartbeatMonitor {
            logger: Logger::new(Some("[HEARTBEAT]")),
            connection_manager,
        }
    }
}

impl Actor for HeartbeatMonitor {
    type Context = Context<Self>;
}
