use crate::connection_manager::ConnectionManager;
use actix::{Actor, Addr, Context, StreamHandler};
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use std::io;

pub struct ServerPeer {
    pub tcp_sender: Addr<TcpSender>,
    pub logger: Logger,
    pub port: u32,
    pub connection_manager: Addr<ConnectionManager>,
}

impl ServerPeer {}

impl Actor for ServerPeer {
    type Context = Context<Self>;
}

impl StreamHandler<Result<String, io::Error>> for ServerPeer {
    fn handle(&mut self, _msg_read: Result<String, io::Error>, _ctx: &mut Self::Context) {
        // match msg_read {
        //     Ok(line_read) => match line_read.strip_suffix("\n") {
        //         Some(line_stripped) => {
        //             self.dispatch_message(line_stripped.to_string(), ctx);
        //         }
        //         None => {
        //             if line_read.is_empty() {
        //                 self.logger.warn("Empty line received");
        //             } else {
        //                 self.dispatch_message(line_read, ctx);
        //             }
        //         }
        //     },
        //     Err(e) => {
        //         self.logger
        //             .error(&format!("Failed to read from stream: {}", e));
        //     }
        // }
    }
}
