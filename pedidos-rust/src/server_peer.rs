use crate::connection_manager::{ConnectionManager, LeaderData, PeerId};
use crate::messages::{
    ElectionCallReceived, ElectionCoordinatorReceived, GetLeaderInfo, GetPeers, LivenessEcho,
    LivenessProbe, PeerDisconnected, PushPendingDeliveryRequest, RemoveOrderInProgressData,
    StartHeartbeat, UpdateCustomerData, UpdateRestaurantData, UpdateRiderData,
};
use actix::{
    Actor, Addr, AsyncContext, Context, Handler, Message, ResponseActFuture, StreamHandler,
    WrapFuture,
};
use actix_async_handler::async_handler;
use common::constants::DEFAULT_PR_HOST;
use common::protocol::{
    ElectionCall, ElectionCoordinator, ElectionOk, SendPopPendingDeliveryRequest,
    SendPushPendingDeliveryRequest, SendRemoveOrderInProgressData, SendUpdateCustomerData,
    SendUpdateOrderInProgressData, SendUpdateRestaurantData, SendUpdateRiderData, SocketMessage,
};
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::{LogLevel, Logger};
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;

#[derive(Message, Debug)]
#[rtype(result = "()")]
struct BeginLivenessCheck {}

#[derive(Message, Debug)]
#[rtype(result = "()")]
struct BeginLivenessCheckWithDelay {
    pub delay: u64,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
struct SendLivenessProbes {
    peers: HashMap<PeerId, Addr<ServerPeer>>,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
struct FinishLivenessCheck {}

#[derive(Clone, Debug)]
pub enum LivenessMarks {
    AliveMark,
    DeadMark,
}

pub struct ServerPeer {
    pub tcp_sender: Addr<TcpSender>,
    pub logger: Logger,
    pub logger_for_heartbeat: Logger,
    pub peer_port: u32,
    pub port: u32,
    pub id: u32,
    pub host_id: u32,
    pub connection_manager: Addr<ConnectionManager>,
    pub udp_socket: Option<Arc<UdpSocket>>,
    pub beat_count: u64,
    pub liveness_marks: Vec<LivenessMarks>,
    pub collecting_liveness_probes: bool,
}

#[async_handler]
impl Handler<StartHeartbeat> for ServerPeer {
    type Result = ();

    async fn handle(&mut self, msg: StartHeartbeat, _ctx: &mut Self::Context) -> Self::Result {
        self.beat_count += 1;
        self.logger_for_heartbeat
            .info(&format!("Starting Heartbeat for peer {}", self.peer_port));
        self.udp_socket = Some(msg.udp_socket);

        _ctx.address().do_send(BeginLivenessCheck {});
    }
}

#[async_handler]
impl Handler<BeginLivenessCheck> for ServerPeer {
    type Result = ();

    async fn handle(&mut self, _msg: BeginLivenessCheck, _ctx: &mut Self::Context) -> Self::Result {
        self.beat_count += 1;
        self.logger_for_heartbeat
            .update_prefix(&format!("[PEER-{} B-{}]", self.peer_port, self.beat_count));

        self.logger_for_heartbeat.debug("BeginLivenessCheck");

        let res_fut = self.connection_manager.send(GetPeers {}).await;
        if let Ok(Ok(peers)) = res_fut {
            _ctx.address().do_send(SendLivenessProbes { peers })
        };
    }
}

impl Handler<SendLivenessProbes> for ServerPeer {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: SendLivenessProbes, _ctx: &mut Self::Context) -> Self::Result {
        self.logger_for_heartbeat.debug("SendLivenessProbes");

        if self.collecting_liveness_probes {
            self.liveness_marks.push(LivenessMarks::DeadMark);
        }

        let my_address = _ctx.address().clone();
        let mut should_early_return = false;

        if self.liveness_marks.len() as u64 >= Self::MAX_LIVENESS_MARKS_TO_DETERMINE_DEAD {
            my_address.do_send(FinishLivenessCheck {});
            should_early_return = true;
        }

        let peers = msg.peers;
        let logger = self.logger_for_heartbeat.clone();
        let udp_socket = self.udp_socket.clone();
        let connection_manager = self.connection_manager.clone();

        self.collecting_liveness_probes = true;
        let my_id = self.id;

        Box::pin(
            async move {
                if (should_early_return) {
                    return;
                }

                let res_fut = connection_manager.send(GetLeaderInfo {}).await;

                if let Ok(Ok(leader_data)) = res_fut {
                    match leader_data {
                        Some(leader_data) => {
                            if leader_data.id == my_id {
                                if let Some(udp_socket) = udp_socket {
                                    let result = Self::send_liveness_probe(
                                        &leader_data,
                                        &peers,
                                        udp_socket,
                                        &logger,
                                        &connection_manager,
                                    )
                                    .await;
                                    if result.is_err() {
                                        logger.warn("Dead detected");
                                    }
                                }
                            }

                            my_address.do_send(BeginLivenessCheckWithDelay {
                                delay: Self::HEARTBEAT_DELAY_IN_SECS,
                            })
                        }
                        None => {
                            logger.info(
                                "No leader detected. Calling elections and delaying liveness check",
                            );
                            connection_manager.do_send(ElectionCall {});
                            my_address.do_send(BeginLivenessCheckWithDelay {
                                delay: Self::HEARTBEAT_LONG_DELAY_IN_SECS,
                            })
                        }
                    }
                };
            }
            .into_actor(self),
        )
    }
}

#[async_handler]
impl Handler<LivenessEcho> for ServerPeer {
    type Result = ();

    async fn handle(&mut self, _msg: LivenessEcho, _ctx: &mut Self::Context) -> Self::Result {
        self.collecting_liveness_probes = false;
        self.liveness_marks.push(LivenessMarks::AliveMark);
    }
}

#[async_handler]
impl Handler<BeginLivenessCheckWithDelay> for ServerPeer {
    type Result = ();

    async fn handle(
        &mut self,
        msg: BeginLivenessCheckWithDelay,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let delay = msg.delay;

        self.logger_for_heartbeat
            .debug("BeginLivenessCheckWithDelay");

        _ctx.notify_later(BeginLivenessCheck {}, Duration::from_secs(delay));
    }
}

impl Handler<FinishLivenessCheck> for ServerPeer {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _msg: FinishLivenessCheck, _ctx: &mut Self::Context) -> Self::Result {
        self.beat_count += 1;

        let liveness_marks = self.liveness_marks.clone();
        self.liveness_marks = Vec::new();

        let connection_manager = self.connection_manager.clone();
        let my_address = _ctx.address().clone();
        let logger = self.logger_for_heartbeat.clone();
        let host_id = self.host_id;

        logger.debug("Finish");

        Box::pin(
            async move {
                Self::report_status_of_peers(&connection_manager, liveness_marks, logger, host_id)
                    .await;
                my_address.do_send(BeginLivenessCheckWithDelay {
                    delay: Self::HEARTBEAT_LONG_DELAY_IN_SECS,
                });
            }
            .into_actor(self),
        )
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

impl Handler<LivenessProbe> for ServerPeer {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: LivenessProbe, _ctx: &mut Self::Context) -> Self::Result {
        let _port = self.port as u16;
        let peer_port = self.peer_port as u16;
        let logger = self.logger_for_heartbeat.clone();
        let socket = msg.udp_socket.clone();

        Box::pin(
            async move {
                let msg_to_send = match Self::serialize_message(&SocketMessage::LivenessProbe) {
                    Ok(ok_result) => ok_result,
                    Err(e) => {
                        logger.error(&format!("Failed to serialize message: {}", e));
                        return;
                    }
                };

                let server_addr: SocketAddr =
                    match format!("{}:{}", DEFAULT_PR_HOST, peer_port).parse() {
                        Ok(addr) => addr,
                        Err(e) => {
                            logger.error(&e.to_string());
                            return;
                        }
                    };

                logger.debug(&format!("Will probe {}", server_addr));

                let timeout_duration = Duration::from_millis(300);
                match tokio::time::timeout(
                    timeout_duration,
                    socket.send_to(msg_to_send.as_bytes(), server_addr),
                )
                .await
                {
                    Ok(a) => {}
                    Err(e) => {
                        logger.error(&format!("Could not send message to UDP socket: {e}"));
                    }
                }
            }
            .into_actor(self),
        )
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
    pub const HEARTBEAT_DELAY_IN_SECS: u64 = 1;
    pub const HEARTBEAT_LONG_DELAY_IN_SECS: u64 = 2;
    pub const MAX_LIVENESS_MARKS_TO_DETERMINE_DEAD: u64 = 2;

    pub fn new(
        host_id: u32,
        id: u32,
        tcp_sender: Addr<TcpSender>,
        port: u32,
        peer_port: u32,
        connection_manager: Addr<ConnectionManager>,
        heart_beat_log_level: LogLevel,
    ) -> ServerPeer {
        let logger_prefix = &format!("[PEER-{peer_port}]");
        let logger = Logger::new(Some(logger_prefix));
        let logger_for_heartbeat = Logger::with_level(Some(logger_prefix), heart_beat_log_level);

        ServerPeer {
            tcp_sender,
            logger,
            logger_for_heartbeat,
            id,
            host_id,
            port,
            peer_port,
            connection_manager,
            udp_socket: None,
            beat_count: 0,
            liveness_marks: Vec::new(),
            collecting_liveness_probes: false,
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

    async fn report_status_of_peers(
        connection_manager: &Addr<ConnectionManager>,
        liveness_marks: Vec<LivenessMarks>,
        logger: Logger,
        host_id: u32,
    ) {
        let mut dead_count = 0;

        for liveness_mark in liveness_marks {
            match liveness_mark {
                LivenessMarks::DeadMark => dead_count += 1,
                LivenessMarks::AliveMark => {}
            }
        }

        let leader_data_res = connection_manager.send(GetLeaderInfo {}).await;

        if let Ok(Ok(leader_data)) = leader_data_res {
            match leader_data {
                Some(leader_data) => {
                    if host_id != leader_data.id {
                        if dead_count >= Self::MAX_LIVENESS_MARKS_TO_DETERMINE_DEAD {
                            connection_manager.do_send(PeerDisconnected {
                                peer_id: leader_data.id,
                            });
                        }
                    }
                }
                None => logger.debug("No leader data found"),
            }
        }
    }

    async fn send_liveness_probe(
        leader_data: &LeaderData,
        peers: &HashMap<PeerId, Addr<ServerPeer>>,
        udp_socket: Arc<UdpSocket>,
        logger: &Logger,
        connection_manager: &Addr<ConnectionManager>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let leader_addr = match peers.get(&leader_data.id) {
            Some(addr) => addr,
            None => {
                let msg = &format!("No peer address found for leader id {}", leader_data.id);
                logger.warn(msg);
                return Err(msg.to_string().into());
            }
        };

        let _ = leader_addr.send(LivenessProbe { udp_socket }).await;
        Ok(())
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
}
