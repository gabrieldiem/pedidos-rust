use crate::connection_manager::ConnectionManager;
use crate::messages::{
    ElectionCallReceived, ElectionCoordinatorReceived, GetLeaderInfo, GotLeaderFromPeer,
    PeerDisconnected, PopPendingDeliveryRequest, PushPendingDeliveryRequest,
    RemoveOrderInProgressData, UpdateCustomerData, UpdateRestaurantData, UpdateRiderData,
};
use actix::{Actor, ActorContext, Addr, AsyncContext, Context, Handler, Message, StreamHandler};
use actix_async_handler::async_handler;
use common::protocol::{
    ElectionCall, ElectionCoordinator, ElectionOk, LeaderQuery, SendPopPendingDeliveryRequest,
    SendPushPendingDeliveryRequest, SendRemoveOrderInProgressData, SendUpdateCustomerData,
    SendUpdateOrderInProgressData, SendUpdatePaymentSystemData, SendUpdateRestaurantData,
    SendUpdateRiderData, SocketMessage, Stop, UpdatePaymentSystemData,
};
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use std::io;
use std::time::Duration;

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct Start {}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct SendLeader {}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct GotLeaderOrCallElection {}

#[allow(dead_code)]
pub struct ServerPeer {
    pub tcp_sender: Addr<TcpSender>,
    pub logger: Logger,
    pub peer_port: u32,
    pub port: u32,
    pub id: u32,
    pub host_id: u32,
    pub connection_manager: Addr<ConnectionManager>,
}

#[allow(clippy::unused_unit)]
#[async_handler]
impl Handler<Stop> for ServerPeer {
    type Result = ();

    async fn handle(&mut self, _msg: Stop, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Stopping peer");
        if self.tcp_sender.connected() {
            self.logger.debug("Stopping TCP sender of peer");
            let _ = self.tcp_sender.send(Stop {}).await;
        }
        _ctx.stop();
    }
}

#[async_handler]
impl Handler<LeaderQuery> for ServerPeer {
    type Result = ();

    async fn handle(&mut self, _msg: LeaderQuery, _ctx: &mut Self::Context) -> Self::Result {
        let msg_to_send = SocketMessage::LeaderQuery;
        self.logger.debug("Querying for leader");

        if let Err(e) = self.send_message(&msg_to_send) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<SendLeader> for ServerPeer {
    type Result = ();

    async fn handle(&mut self, _msg: SendLeader, _ctx: &mut Self::Context) -> Self::Result {
        let res_fut = self.connection_manager.send(GetLeaderInfo {}).await;

        if let Ok(Ok(leader_data)) = res_fut {
            match leader_data {
                Some(leader_data) => {
                    let msg_to_send = SocketMessage::LeaderData(leader_data.port);
                    self.logger.debug("Sending leader info");

                    if let Err(e) = self.send_message(&msg_to_send) {
                        self.logger.error(&e.to_string());
                        return;
                    }
                }
                None => {
                    self.logger.debug("I don't know the leader");
                }
            }
        };
    }
}

#[async_handler]
impl Handler<GotLeaderOrCallElection> for ServerPeer {
    type Result = ();

    async fn handle(
        &mut self,
        _msg: GotLeaderOrCallElection,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let res_fut = self.connection_manager.send(GetLeaderInfo {}).await;

        if let Ok(Ok(leader_data)) = res_fut {
            match leader_data {
                Some(_leader_data) => {
                    self.logger.debug("Leader already found");
                }
                None => {
                    self.logger.debug("Leader not found. Calling elections");
                    _ctx.address().do_send(ElectionCall {});
                }
            }
        };
    }
}

#[async_handler]
impl Handler<Start> for ServerPeer {
    type Result = ();

    async fn handle(&mut self, _msg: Start, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Starting peer");

        self.connection_manager.do_send(LeaderQuery {});
        _ctx.notify_later(GotLeaderOrCallElection {}, Duration::from_secs(2));
    }
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
            .debug(&format!("Sending ElectionOk to {}", self.peer_port));

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
            .debug(&format!("Sending ElectionCall to {}", self.peer_port));

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
            self.peer_port
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
        self.logger.debug(&format!(
            "Sending ElectionCoordinator to {}",
            self.peer_port
        ));

        if let Err(e) = self.send_message(&msg_to_send) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<SendUpdatePaymentSystemData> for ServerPeer {
    type Result = ();

    async fn handle(
        &mut self,
        msg: SendUpdatePaymentSystemData,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(e) = self.send_message(&SocketMessage::UpdatePaymentSystemData(msg.port)) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<SendUpdateCustomerData> for ServerPeer {
    type Result = ();

    async fn handle(
        &mut self,
        msg: SendUpdateCustomerData,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(e) = self.send_message(&SocketMessage::UpdateCustomerData(
            msg.customer_id,
            msg.location,
            msg.order_price,
        )) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<SendUpdateRestaurantData> for ServerPeer {
    type Result = ();

    async fn handle(
        &mut self,
        msg: SendUpdateRestaurantData,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(e) = self.send_message(&SocketMessage::UpdateRestaurantData(
            msg.restaurant_name,
            msg.location,
        )) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<SendUpdateRiderData> for ServerPeer {
    type Result = ();

    async fn handle(&mut self, msg: SendUpdateRiderData, _ctx: &mut Self::Context) -> Self::Result {
        if let Err(e) =
            self.send_message(&SocketMessage::UpdateRiderData(msg.rider_id, msg.location))
        {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<SendUpdateOrderInProgressData> for ServerPeer {
    type Result = ();

    async fn handle(
        &mut self,
        msg: SendUpdateOrderInProgressData,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(e) = self.send_message(&SocketMessage::UpdateOrderInProgressData(
            msg.customer_id,
            msg.customer_location,
            msg.order_price,
            msg.rider_id,
        )) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<SendRemoveOrderInProgressData> for ServerPeer {
    type Result = ();

    async fn handle(
        &mut self,
        msg: SendRemoveOrderInProgressData,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(e) =
            self.send_message(&SocketMessage::RemoveOrderInProgressData(msg.customer_id))
        {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<SendPushPendingDeliveryRequest> for ServerPeer {
    type Result = ();

    async fn handle(
        &mut self,
        msg: SendPushPendingDeliveryRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(e) = self.send_message(&SocketMessage::PushPendingDeliveryRequest(
            msg.customer_id,
            msg.restaurant_location,
            msg.to_front,
        )) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<SendPopPendingDeliveryRequest> for ServerPeer {
    type Result = ();

    async fn handle(
        &mut self,
        _msg: SendPopPendingDeliveryRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if let Err(e) = self.send_message(&SocketMessage::PopPendingDeliveryRequest) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

impl ServerPeer {
    pub fn new(
        host_id: u32,
        id: u32,
        tcp_sender: Addr<TcpSender>,
        port: u32,
        peer_port: u32,
        connection_manager: Addr<ConnectionManager>,
    ) -> ServerPeer {
        let logger_prefix = &format!("[PEER-{peer_port}]");
        let logger = Logger::new(Some(logger_prefix));

        ServerPeer {
            tcp_sender,
            logger,
            id,
            host_id,
            port,
            peer_port,
            connection_manager,
        }
    }

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
                        leader_port: self.peer_port,
                    });
                }
                SocketMessage::UpdateCustomerData(customer_id, location, order_price) => {
                    self.logger
                        .info(&format!("Updating data for customer {customer_id}"));
                    self.connection_manager.do_send(UpdateCustomerData {
                        customer_id,
                        location,
                        order_price,
                    })
                }
                SocketMessage::UpdateRestaurantData(restaurant_name, location) => {
                    self.logger
                        .info(&format!("Updating data for restaurant {restaurant_name}"));
                    self.connection_manager.do_send(UpdateRestaurantData {
                        restaurant_name,
                        location,
                    })
                }
                SocketMessage::UpdateRiderData(rider_id, location) => {
                    self.logger
                        .info(&format!("Updating data for rider {rider_id}"));
                    self.connection_manager
                        .do_send(UpdateRiderData { rider_id, location })
                }
                SocketMessage::UpdatePaymentSystemData(port) => {
                    self.logger
                        .info(&format!("Updating data for payment system {port}"));
                    self.connection_manager
                        .do_send(UpdatePaymentSystemData { port })
                }
                SocketMessage::RemoveOrderInProgressData(customer_id) => {
                    self.logger.info(&format!(
                        "Removing data for order from customer {customer_id}"
                    ));
                    self.connection_manager
                        .do_send(RemoveOrderInProgressData { customer_id })
                }
                SocketMessage::PushPendingDeliveryRequest(
                    customer_id,
                    restaurant_location,
                    to_front,
                ) => {
                    self.logger.info(&format!(
                        "Removing data for order from customer {customer_id}"
                    ));
                    self.connection_manager.do_send(PushPendingDeliveryRequest {
                        customer_id,
                        restaurant_location,
                        to_front,
                    })
                }
                SocketMessage::LeaderQuery => {
                    ctx.address().do_send(SendLeader {});
                }
                SocketMessage::LeaderData(leader_port) => {
                    self.connection_manager
                        .do_send(GotLeaderFromPeer { leader_port });
                }
                SocketMessage::PopPendingDeliveryRequest => self
                    .connection_manager
                    .do_send(PopPendingDeliveryRequest {}),
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

    fn serialize_message(
        socket_message: &SocketMessage,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let msg_to_send = match serde_json::to_string(socket_message) {
            Ok(ok_result) => ok_result,
            Err(e) => {
                return Err(format!("Failed to serialize message: {}", e).into());
            }
        };

        Ok(msg_to_send)
    }

    fn send_message(&self, socket_message: &SocketMessage) -> Result<(), String> {
        // message serialization
        let msg_to_send = match Self::serialize_message(socket_message) {
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

    fn finished(&mut self, _ctx: &mut Self::Context) {
        self.logger
            .warn(&format!("Detected peer with id {} down", self.id));
        self.connection_manager
            .do_send(PeerDisconnected { peer_id: self.id });
    }
}
