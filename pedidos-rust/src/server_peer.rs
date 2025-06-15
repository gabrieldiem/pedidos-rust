use crate::connection_manager::ConnectionManager;
use crate::messages::{ElectionCallReceived, ElectionCoordinatorReceived};
use actix::{Actor, Addr, AsyncContext, Context, Handler, StreamHandler};
use actix_async_handler::async_handler;
use common::protocol::{ElectionCall, ElectionCoordinator, ElectionOk, SocketMessage};
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use std::io;

pub struct ServerPeer {
    pub tcp_sender: Addr<TcpSender>,
    pub logger: Logger,
    pub port: u32,
    pub connection_manager: Addr<ConnectionManager>,
}

#[async_handler]
impl Handler<ElectionCallReceived> for ServerPeer {
    type Result = ();

    async fn handle(
        &mut self,
        _msg: ElectionCallReceived,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.connection_manager.do_send(ElectionCall {});

        let msg_to_send = SocketMessage::ElectionOk;
        self.logger
            .debug(&format!("Sending ElectionOk to {}", self.port));

        if let Err(e) = self.send_message(&msg_to_send) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<ElectionCall> for ServerPeer {
    type Result = ();

    async fn handle(&mut self, _msg: ElectionCall, _ctx: &mut Self::Context) -> Self::Result {
        let msg_to_send = SocketMessage::ElectionCall;
        self.logger
            .debug(&format!("Sending ElectionCall to {}", self.port));

        if let Err(e) = self.send_message(&msg_to_send) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<ElectionOk> for ServerPeer {
    type Result = ();

    async fn handle(&mut self, _msg: ElectionOk, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Received ElectionOk from {}. Waiting for coordinator message",
            self.port
        ));
    }
}

#[async_handler]
impl Handler<ElectionCoordinatorReceived> for ServerPeer {
    type Result = ();

    async fn handle(
        &mut self,
        msg: ElectionCoordinatorReceived,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.connection_manager
            .do_send(ElectionCoordinatorReceived {
                leader_port: msg.leader_port,
            });
    }
}

#[async_handler]
impl Handler<ElectionCoordinator> for ServerPeer {
    type Result = ();

    async fn handle(
        &mut self,
        _msg: ElectionCoordinator,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let msg_to_send = SocketMessage::ElectionCoordinator;
        self.logger
            .debug(&format!("Sending ElectionCoordinator to {}", self.port));

        if let Err(e) = self.send_message(&msg_to_send) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

impl ServerPeer {
    #[allow(unreachable_patterns)]
    fn dispatch_message(&mut self, line_read: String, ctx: &mut <ServerPeer as Actor>::Context) {
        let parsed_line = serde_json::from_str(&line_read);
        match parsed_line {
            Ok(message) => match message {
                SocketMessage::ElectionCall => {
                    ctx.address().do_send(ElectionCallReceived {});
                }
                SocketMessage::ElectionOk => {
                    ctx.address().do_send(ElectionOk {});
                }
                SocketMessage::ElectionCoordinator => {
                    ctx.address().do_send(ElectionCoordinatorReceived {
                        leader_port: self.port,
                    });
                }
                _ => {
                    self.logger
                        .warn(&format!("Unrecognized message: {:?}", message));
                }
            },
            Err(e) => {
                self.logger.error(&format!(
                    "Failed to deserialize message: {}. Message received: {}",
                    e, line_read
                ));
            }
        }
    }

    fn send_message(&self, socket_message: &SocketMessage) -> Result<(), String> {
        // message serialization
        let msg_to_send = match serde_json::to_string(socket_message) {
            Ok(ok_result) => ok_result,
            Err(e) => {
                return Err(format!("Failed to serialize message: {}", e));
            }
        };

        // sending message
        if let Err(e) = self.tcp_sender.try_send(TcpMessage {
            data: msg_to_send + "\n",
        }) {
            return Err(format!("Failed to write to stream: {}", e));
        }

        Ok(())
    }
}

impl Actor for ServerPeer {
    type Context = Context<Self>;
}

impl StreamHandler<Result<String, io::Error>> for ServerPeer {
    fn handle(&mut self, msg_read: Result<String, io::Error>, ctx: &mut Self::Context) {
        match msg_read {
            Ok(line_read) => match line_read.strip_suffix("\n") {
                Some(line_stripped) => {
                    self.dispatch_message(line_stripped.to_string(), ctx);
                }
                None => {
                    if line_read.is_empty() {
                        self.logger.warn("Empty line received");
                    } else {
                        self.dispatch_message(line_read, ctx);
                    }
                }
            },
            Err(e) => {
                self.logger
                    .error(&format!("Failed to read from stream: {}", e));
            }
        }
    }
}
