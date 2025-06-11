use crate::connection_manager::ConnectionManager;
use crate::messages::{FindRider, RegisterCustomer, RegisterRestaurant, RegisterRider};
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, StreamHandler};
use actix_async_handler::async_handler;
use common::protocol::{
    DeliveryDone, DeliveryOffer, DeliveryOfferAccepted, FinishDelivery, Location, LocationUpdate,
    Order, PushNotification, RiderArrivedAtCustomer, SocketMessage,
};
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use std::io;

pub struct ClientConnection {
    pub tcp_sender: Addr<TcpSender>,
    pub logger: Logger,
    pub id: u32, // the id is the port
    pub connection_manager: Addr<ConnectionManager>,
    pub peer_location: Option<Location>,
}

impl Actor for ClientConnection {
    type Context = Context<Self>;
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct SendRestaurants {
    customer_location: Location,
}

#[async_handler]
impl Handler<SendRestaurants> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, msg: SendRestaurants, _ctx: &mut Self::Context) -> Self::Result {
        let res = self.connection_manager.send(RegisterCustomer {
            id: self.id,
            location: msg.customer_location,
            address: _ctx.address(),
        });

        let res_awaited = res.await;
        if let Err(e) = res_awaited {
            self.logger
                .error(&format!("Failed to register customer: {}", e));
            return;
        }

        self.logger.debug("Sending Restaurants");

        let restaurantes =
            serde_json::to_string(&vec!["McDonalds", "BurgerKing", "Wendys"]).unwrap();

        let msg = SocketMessage::Restaurants(restaurantes);

        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
        }
    }
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct RegisterNewRestaurant {
    restaurant_location: Location,
    name: String,
}

#[async_handler]
impl Handler<RegisterNewRestaurant> for ClientConnection {
    type Result = ();

    async fn handle(
        &mut self,
        msg: RegisterNewRestaurant,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let res = self.connection_manager.send(RegisterRestaurant {
            name: msg.name,
            id: self.id,
            location: msg.restaurant_location,
            address: _ctx.address(),
        });

        let res_awaited = res.await;
        if let Err(e) = res_awaited {
            self.logger
                .error(&format!("Failed to register restaurant: {}", e));
            return;
        }
        self.logger.debug("New restaurant registered");
    }
}

#[async_handler]
impl Handler<Order> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, msg: Order, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Processing order of {} from {}",
            msg.order.amount, msg.order.restaurant
        ));

        let msg = SocketMessage::PushNotification("Processing order".to_owned());
        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
            return;
        }

        // Enviar el mensaje AuthorizePayment al Payment System, y con el handler de PaymentAuthorized
        // recién ahí se debería enviar un mensaje al restaurante.
        // Para eso, debería enviar un mensaje a ConnectionManager para que él envie AuthorizePayment

        self.connection_manager.do_send(FindRider {
            customer_id: self.id,
        });
    }
}

#[async_handler]
impl Handler<PushNotification> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, msg: PushNotification, _ctx: &mut Self::Context) -> Self::Result {
        let msg = SocketMessage::PushNotification(msg.notification_msg);
        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<LocationUpdate> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, msg: LocationUpdate, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Rider is now at ({}, {})",
            msg.new_location.x, msg.new_location.y
        ));

        if self.peer_location.is_none() {
            let msg = RegisterRider {
                id: self.id,
                address: _ctx.address(),
            };
            self.connection_manager.do_send(msg);
        }

        self.peer_location = Some(msg.new_location);
    }
}

#[async_handler]
impl Handler<DeliveryOffer> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, msg: DeliveryOffer, _ctx: &mut Self::Context) -> Self::Result {
        let msg = SocketMessage::DeliveryOffer(msg.customer_id, msg.customer_location);
        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<DeliveryOfferAccepted> for ClientConnection {
    type Result = ();

    async fn handle(
        &mut self,
        msg: DeliveryOfferAccepted,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.connection_manager.do_send(msg);
    }
}

#[async_handler]
impl Handler<RiderArrivedAtCustomer> for ClientConnection {
    type Result = ();

    async fn handle(
        &mut self,
        msg: RiderArrivedAtCustomer,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.connection_manager.do_send(msg);
    }
}

#[async_handler]
impl Handler<DeliveryDone> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, msg: DeliveryDone, _ctx: &mut Self::Context) -> Self::Result {
        self.connection_manager.do_send(msg);
    }
}

#[async_handler]
impl Handler<FinishDelivery> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, _msg: FinishDelivery, _ctx: &mut Self::Context) -> Self::Result {
        if let Err(e) = self.send_message(&SocketMessage::FinishDelivery) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

impl ClientConnection {
    #[allow(unreachable_patterns)]
    fn dispatch_message(
        &mut self,
        line_read: String,
        ctx: &mut <ClientConnection as Actor>::Context,
    ) {
        let parsed_line = serde_json::from_str(&line_read);
        match parsed_line {
            Ok(message) => match message {
                SocketMessage::GetRestaurants(customer_location) => {
                    self.logger.debug("Got request for GetRestaurants");
                    ctx.address().do_send(SendRestaurants { customer_location });
                }
                SocketMessage::Order(order) => {
                    self.logger.debug("Got order request");
                    ctx.address().do_send(Order { order });
                }
                SocketMessage::LocationUpdate(new_location) => {
                    self.logger.debug("Got a location update");
                    ctx.address().do_send(LocationUpdate { new_location });
                }
                SocketMessage::DeliveryOfferAccepted(customer_id) => {
                    self.logger.debug("Rider accepted delivery offer");
                    ctx.address().do_send(DeliveryOfferAccepted {
                        customer_id,
                        rider_id: self.id,
                    });
                }
                SocketMessage::RiderArrivedAtCustomer => {
                    self.logger.debug("Rider arrived at customer location");
                    ctx.address()
                        .do_send(RiderArrivedAtCustomer { rider_id: self.id });
                }
                SocketMessage::DeliveryDone => {
                    self.logger.debug("Rider finished the delivery");
                    ctx.address().do_send(DeliveryDone { rider_id: self.id });
                }
                SocketMessage::InformLocation(location, name) => {
                    self.logger.debug("A new restaurant wants to register");
                    ctx.address().do_send(RegisterNewRestaurant {
                        restaurant_location: location,
                        name: name.to_string(),
                    });
                }
                _ => {
                    self.logger
                        .warn(&format!("Unrecognized message: {:?}", message));
                }
            },
            Err(e) => {
                self.logger
                    .error(&format!("Failed to deserialize message: {}", e));
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

impl StreamHandler<Result<String, io::Error>> for ClientConnection {
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
