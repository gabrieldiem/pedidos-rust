use crate::connection_manager::ConnectionManager;
use crate::messages::{
    AuthorizePayment, OrderCancelled, OrderReady, PaymentAuthorized, PaymentDenied,
    PaymentExecuted, RegisterPaymentSystem, SendNotification,
};
use crate::messages::{RegisterCustomer, RegisterRestaurant, RegisterRider, SendRestaurantList};
use actix::{
    Actor, ActorContext, Addr, AsyncContext, Context, Handler, Message, ResponseActFuture,
    StreamHandler, WrapFuture,
};
use actix_async_handler::async_handler;
use common::protocol::{
    AuthorizePaymentRequest, DeliveryDone, DeliveryOffer, DeliveryOfferAccepted, ExecutePayment,
    FinishDelivery, Location, LocationUpdate, Order, OrderInProgress, OrderToRestaurant,
    PushNotification, Restaurants, RiderArrivedAtCustomer, SocketMessage, Stop,
};
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use std::io;

pub struct ClientConnection {
    pub is_leader: bool,
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

// No uso #[async_handler] porque, al hacer dos llamadas a send . await hay múltiples combinaciones
// de futures de Actix dentro de un async fn, y eso no puede resolverse bien en tiempo de
// compilación porque no implementan el trait ActorFuture. El #[async_handler] es como un decorator
// pero tiene limitaciones en cuanto a la complejidad de los futures que se pueden usar dentro de él.
// Por eso, uso el Box::pin(fut.into_actor(self)) para poder usar múltiples futures dentro de un async fn.
impl Handler<SendRestaurants> for ClientConnection {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: SendRestaurants, ctx: &mut Self::Context) -> Self::Result {
        let addr = ctx.address();
        let logger = self.logger.clone();
        let connection_manager = self.connection_manager.clone();
        let id = self.id;

        let fut = async move {
            let res = connection_manager
                .send(RegisterCustomer {
                    id,
                    location: msg.customer_location,
                    address: addr,
                })
                .await;

            if let Err(e) = res {
                logger.error(&format!("Failed to register customer: {}", e));
                return;
            }

            logger.debug("Sending Restaurants");

            let res2 = connection_manager
                .send(SendRestaurantList { customer_id: id })
                .await;

            if let Err(e) = res2 {
                logger.error(&format!("Failed to register restaurant: {}", e));
            }
        };

        Box::pin(fut.into_actor(self))
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
impl Handler<Restaurants> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, msg: Restaurants, _ctx: &mut Self::Context) -> Self::Result {
        let msg_to_send = SocketMessage::Restaurants(msg.data);
        self.logger.debug(&format!(
            "Sending restaurant list to client {}: {:?}",
            self.id, msg_to_send
        ));
        if let Err(e) = self.send_message(&msg_to_send) {
            self.logger.error(&e.to_string());
            return;
        }
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
        self.connection_manager.do_send(AuthorizePayment {
            customer_id: self.id,
            price: msg.order.amount,
            restaurant_name: msg.order.restaurant.clone(),
        });
    }
}

#[async_handler]
impl Handler<AuthorizePaymentRequest> for ClientConnection {
    type Result = ();

    async fn handle(
        &mut self,
        msg: AuthorizePaymentRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.logger.debug(&format!(
            "Sending AuthorizePayment to payment system for customer {} with price {}",
            msg.customer_id, msg.price
        ));

        let msg = SocketMessage::AuthorizePayment(msg.customer_id, msg.price, msg.restaurant_name);
        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<ExecutePayment> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, msg: ExecutePayment, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Executing payment with payment system for customer {} with price {}",
            msg.customer_id, msg.price
        ));

        let msg = SocketMessage::ExecutePayment(msg.customer_id, msg.price);
        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<OrderToRestaurant> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, msg: OrderToRestaurant, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Sending order from client {}: {:?}",
            msg.customer_id, msg.price
        ));

        let msg = SocketMessage::PrepareOrder(msg.customer_id, msg.price);
        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<OrderInProgress> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, msg: OrderInProgress, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Order in progress for customer {}",
            msg.customer_id
        ));

        let message = SendNotification {
            message: "Your order is being prepared by the restaurant".to_string(),
            recipient_id: msg.customer_id,
        };
        self.connection_manager.do_send(message);
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
        if let Err(e) = self.send_message(&SocketMessage::FinishDelivery(_msg.reason)) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<Stop> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, _msg: Stop, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Stopping connection");
        _ctx.stop();
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
                SocketMessage::OrderInProgress(client_id) => {
                    ctx.address().do_send(OrderInProgress {
                        customer_id: client_id,
                    });
                }
                SocketMessage::OrderReady(client_id) => {
                    self.logger
                        .debug(&format!("Order ready for client {}", client_id));
                    self.connection_manager.do_send(OrderReady {
                        customer_id: client_id,
                    })
                }
                SocketMessage::OrderCalcelled(client_id) => {
                    self.logger
                        .debug(&format!("Order cancelled for client {}", client_id));
                    self.connection_manager.do_send(OrderCancelled {
                        customer_id: client_id,
                    });
                }
                SocketMessage::RegisterPaymentSystem => {
                    self.connection_manager.do_send(RegisterPaymentSystem {
                        address: ctx.address(),
                    });
                }
                SocketMessage::PaymentAuthorized(customer_id, amount, restaurant_name) => {
                    self.connection_manager.do_send(PaymentAuthorized {
                        customer_id,
                        amount,
                        restaurant_name,
                    })
                }
                SocketMessage::PaymentDenied(customer_id, amount, restaurant_name) => {
                    self.connection_manager.do_send(PaymentDenied {
                        customer_id,
                        amount,
                        restaurant_name,
                    })
                }
                SocketMessage::PaymentExecuted(customer_id, amount) => {
                    self.connection_manager.do_send(PaymentExecuted {
                        customer_id,
                        amount,
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

    /// Envía un mensaje serializado a través del TcpSender asociado.
    ///
    /// # Argumentos
    /// * `socket_message` - Referencia al mensaje de tipo `SocketMessage` que se enviará.
    ///
    /// # Retorna
    /// * `Ok(())` si el mensaje fue serializado y enviado correctamente.
    /// * `Err(String)` si ocurre un error al serializar el mensaje o al enviarlo por el stream.
    ///
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
