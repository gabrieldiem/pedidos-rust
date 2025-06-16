use crate::client_connection::ClientConnection;
use crate::messages::{
    AuthorizePayment, ElectionCoordinatorReceived, FindRider, GetLeaderInfo, GetPeers,
    IsPeerConnected, OrderCancelled, OrderReady, OrderRequest, PaymentAuthorized, PaymentDenied,
    PaymentExecuted, RegisterCustomer, RegisterPaymentSystem, RegisterPeerServer,
    RegisterRestaurant, RegisterRider, SendNotification, SendRestaurantList, UpdateCustomerData,
};
use crate::nearby_entitys::NearbyEntities;
use crate::server_peer::ServerPeer;
use actix::{Actor, Addr, AsyncContext, Context, Handler, ResponseActFuture, WrapFuture};
use actix_async_handler::async_handler;
use common::configuration::Configuration;
use common::constants::{N_RIDERS_TO_NOTIFY, NO_RESTAURANTS};
use common::protocol::{
    AuthorizePaymentRequest, DeliveryDone, DeliveryOffer, DeliveryOfferAccepted,
    DeliveryOfferConfirmed, ElectionCall, ElectionCoordinator, ExecutePayment, FinishDelivery,
    Location, OrderToRestaurant, PushNotification, Restaurants, RiderArrivedAtCustomer,
    SendUpdateCustomerData,
};
use common::utils::logger::Logger;
use std::collections::{HashMap, VecDeque};

type CustomerId = u32;
type RiderId = u32;
type RestaurantName = String;

#[derive(Debug, Clone)]
pub struct LeaderData {
    pub id: u32,
    pub port: u32,
}

#[derive(Debug, Clone)]
pub struct RiderData {
    pub address: Addr<ClientConnection>,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Copy)]
pub struct OrderData {
    pub rider_id: Option<RiderId>,
    pub order_price: Option<f64>,
    pub customer_location: Location,
    pub customer_id: CustomerId,
}

#[derive(Debug, Clone)]
pub struct CustomerData {
    pub location: Location,
    pub order_price: Option<f64>,
}

impl CustomerData {
    pub fn new(location: Location, order_price: Option<f64>) -> CustomerData {
        CustomerData {
            location,
            order_price,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RestaurantData {
    pub address: Addr<ClientConnection>,
    pub location: Location,
}

impl RestaurantData {
    pub fn new(address: Addr<ClientConnection>, location: Location) -> RestaurantData {
        RestaurantData { address, location }
    }
}

pub type PeerId = u32;

pub struct ConnectionManager {
    pub logger: Logger,
    pub id: u32,
    pub port: u32,
    pub configuration: Configuration,

    // Entities
    pub customer_connections: HashMap<CustomerId, Addr<ClientConnection>>,
    pub riders: HashMap<RiderId, RiderData>,
    pub customers: HashMap<CustomerId, CustomerData>,
    pub restaurants: HashMap<RestaurantName, RestaurantData>,
    pub payment_system: Option<Addr<ClientConnection>>,

    pub orders_in_process: HashMap<CustomerId, OrderData>,
    pub pending_delivery_requests: VecDeque<FindRider>,

    // Peers
    pub next_server_peer: Option<(PeerId, Addr<ServerPeer>)>,
    pub server_peers: HashMap<PeerId, Addr<ServerPeer>>,
    pub leader: Option<LeaderData>,
    pub election_in_progress: bool,
}

impl ConnectionManager {
    pub fn new(id: u32, port: u32, configuration: Configuration) -> ConnectionManager {
        ConnectionManager {
            id,
            port,
            configuration,
            logger: Logger::new(Some("[CONNECTION-MANAGER]")),
            riders: HashMap::new(),
            customer_connections: HashMap::new(),
            customers: HashMap::new(),
            orders_in_process: HashMap::new(),
            pending_delivery_requests: VecDeque::new(),
            restaurants: HashMap::new(),
            payment_system: None,
            server_peers: HashMap::new(),
            leader: None,
            next_server_peer: None,
            election_in_progress: false,
        }
    }

    fn process_pending_requests(&mut self) {
        while let Some(pending_request) = self.pending_delivery_requests.pop_front() {
            if let Some(rider) = self.riders.values().find(|_rider| true) {
                self.logger
                    .debug("Assigned pending delivery request to newly registered rider");
                match self.customers.get(&pending_request.customer_id) {
                    Some(customer) => {
                        rider.address.do_send(DeliveryOffer {
                            customer_id: pending_request.customer_id,
                            customer_location: customer.location,
                        });
                    }
                    None => {
                        self.logger
                            .warn("Failed to find customer data when finding rider");
                    }
                }
            } else {
                self.pending_delivery_requests.push_front(pending_request);
                break;
            }
        }
    }
}

impl Actor for ConnectionManager {
    type Context = Context<Self>;
}

#[async_handler]
impl Handler<GetPeers> for ConnectionManager {
    type Result = Result<HashMap<PeerId, Addr<ServerPeer>>, ()>;

    async fn handle(&mut self, _msg: GetPeers, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.server_peers.clone())
    }
}

#[async_handler]
impl Handler<ElectionCoordinatorReceived> for ConnectionManager {
    type Result = ();

    async fn handle(
        &mut self,
        msg: ElectionCoordinatorReceived,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.election_in_progress = false;
        let leader_port = msg.leader_port;

        let info = self
            .configuration
            .pedidos_rust
            .infos
            .iter()
            .find(|pair| pair.port == leader_port);

        match info {
            Some(info) => {
                self.leader = Some(LeaderData {
                    id: info.id,
                    port: leader_port,
                });
                self.logger.debug(&format!(
                    "Election coordinator received. New leader is {} with ID {}",
                    leader_port, info.id
                ));
            }
            None => self
                .logger
                .debug(&format!("No info for port {leader_port}")),
        }
    }
}

#[async_handler]
impl Handler<ElectionCall> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, _msg: ElectionCall, _ctx: &mut Self::Context) -> Self::Result {
        if self.election_in_progress {
            return;
        }

        // Send to all peers with higher ID an ElectionCall message
        for peer_id in self.server_peers.keys() {
            if *peer_id > self.id {
                match self.server_peers.get(peer_id) {
                    Some(peer_addr) => {
                        peer_addr.do_send(ElectionCall {});
                        self.election_in_progress = true;
                    }
                    None => {
                        self.logger.warn("No peer found");
                    }
                };
            }
        }

        // If no message was sent, then I am the highest currently
        // So I am the leader
        if !self.election_in_progress {
            self.leader = Some(LeaderData {
                id: self.id,
                port: self.port,
            });

            for peer_id in self.server_peers.keys() {
                match self.server_peers.get(peer_id) {
                    Some(peer_addr) => {
                        peer_addr.do_send(ElectionCoordinator {});
                    }
                    None => {
                        self.logger.warn("No peer found");
                    }
                };
            }
        }
    }
}

#[async_handler]
impl Handler<GetLeaderInfo> for ConnectionManager {
    type Result = Result<Option<LeaderData>, ()>;

    async fn handle(&mut self, _msg: GetLeaderInfo, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.leader.clone())
    }
}

#[async_handler]
impl Handler<RegisterPeerServer> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: RegisterPeerServer, _ctx: &mut Self::Context) -> Self::Result {
        self.logger
            .debug(&format!("Registering server peer with ID {}", msg.id));
        self.server_peers
            .entry(msg.id)
            .or_insert(msg.address.clone());

        let (lesser_peers, greater_peers): (Vec<&u32>, Vec<&u32>) = self
            .server_peers
            .keys()
            .into_iter()
            .partition(|&n| *n <= self.id);
        let updated_next_peer_id;
        if greater_peers.is_empty() {
            updated_next_peer_id = *lesser_peers.iter().min().unwrap_or(&&0);
        } else {
            updated_next_peer_id = *greater_peers.iter().min().unwrap_or(&&0);
        }
        self.next_server_peer = Some((
            *updated_next_peer_id,
            self.server_peers
                .get(&updated_next_peer_id)
                .unwrap() // already checked the peer exists before
                .clone(),
        ));
        self.logger.info(&format!(
            "Updated next peer server to peer no {updated_next_peer_id}"
        ));
        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<IsPeerConnected> for ConnectionManager {
    type Result = Result<bool, ()>;

    async fn handle(&mut self, msg: IsPeerConnected, _ctx: &mut Self::Context) -> Self::Result {
        self.logger
            .debug(&format!("Checking if peer with ID {} is connected", msg.id));

        return match self.server_peers.get(&msg.id) {
            Some(_server_peer) => {
                self.logger
                    .debug(&format!("Peer with ID {} is connected", msg.id));
                Ok(true)
            }
            None => {
                self.logger
                    .debug(&format!("Peer with ID {} is not connected", msg.id));
                self.process_pending_requests();
                Ok(false)
            }
        };
    }
}

#[async_handler]
impl Handler<RegisterRider> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: RegisterRider, _ctx: &mut Self::Context) -> Self::Result {
        self.logger
            .debug(&format!("Registering Rider with ID {}", msg.id));
        self.riders.entry(msg.id).or_insert(RiderData {
            address: msg.address,
            location: Some(msg.location),
        });
        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<RegisterCustomer> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: RegisterCustomer, _ctx: &mut Self::Context) -> Self::Result {
        self.logger
            .debug(&format!("Registering Customer with ID {}", msg.id));
        self.customers
            .entry(msg.id)
            .or_insert(CustomerData::new(msg.location, None));
        self.customer_connections
            .entry(msg.id)
            .or_insert(msg.address);
        if let Some((_, peer)) = &self.next_server_peer {
            peer.do_send(SendUpdateCustomerData {
                customer_id: msg.id,
                location: msg.location,
                order_price: None,
            });
        }
        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<RegisterRestaurant> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: RegisterRestaurant, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Registering Restaurant with ID {} and name {}",
            msg.id, msg.name
        ));
        self.restaurants
            .entry(msg.name)
            .or_insert(RestaurantData::new(msg.address, msg.location));
        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<RegisterPaymentSystem> for ConnectionManager {
    type Result = ();

    async fn handle(
        &mut self,
        msg: RegisterPaymentSystem,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.logger.debug("Registering Payment System");
        self.payment_system = Some(msg.address);
        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<SendRestaurantList> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: SendRestaurantList, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Sending available restaurants to client {}",
            msg.customer_id
        ));

        match self.customers.get(&msg.customer_id) {
            Some(customer) => {
                let nearby =
                    NearbyEntities::nearby_restaurants(&customer.location, &self.restaurants);

                let restaurant_list = if nearby.is_empty() {
                    NO_RESTAURANTS.to_string()
                } else {
                    nearby.into_iter().cloned().collect::<Vec<_>>().join(", ")
                };

                self.logger
                    .debug(&format!("Restaurant list to send: {}", restaurant_list));

                match self.customer_connections.get(&msg.customer_id) {
                    Some(customer_address) => customer_address.do_send(Restaurants {
                        data: restaurant_list,
                    }),
                    None => {
                        self.logger
                            .warn("Failed to find customer data when sending restaurant list");
                    }
                }
            }
            None => {
                self.logger
                    .warn("Failed to find customer data when sending restaurant list");
            }
        }
        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<AuthorizePayment> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: AuthorizePayment, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Authorizing payment for customer {} with price {}",
            msg.customer_id, msg.price
        ));

        let msg_to_send = AuthorizePaymentRequest {
            customer_id: msg.customer_id,
            price: msg.price,
            restaurant_name: msg.restaurant_name.clone(),
        };

        if let Some(payment_system) = &self.payment_system {
            payment_system.do_send(msg_to_send);
        } else {
            self.logger
                .warn("Failed to find payment system when authorizing payment");
            if let Some(customer_address) = self.customer_connections.get(&msg.customer_id) {
                customer_address.do_send(FinishDelivery {
                    reason: "Payment system is not available, please try again later.".to_string(),
                });
            } else {
                self.logger
                    .warn("Failed to find customer data when authorizing payment");
            }
        }
        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<PaymentAuthorized> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: PaymentAuthorized, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Payment authorized for customer {} with amount {}",
            msg.customer_id, msg.amount
        ));

        if let Some(customer_adress) = self.customer_connections.get(&msg.customer_id) {
            customer_adress.do_send(PushNotification {
                notification_msg: format!(
                    "Payment of {} authorized for your order at {}",
                    msg.amount, msg.restaurant_name
                ),
            });

            _ctx.address().do_send(OrderRequest {
                customer_id: msg.customer_id,
                restaurant_name: msg.restaurant_name.clone(),
                order_price: msg.amount,
            });
        } else {
            self.logger
                .warn("Failed to find customer data when authorizing payment");
        }

        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<PaymentDenied> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: PaymentDenied, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Payment denied for customer {} with amount {}",
            msg.customer_id, msg.amount
        ));

        if let Some(customer_address) = self.customer_connections.get(&msg.customer_id) {
            let reason_msg = format!(
                "Payment of {} denied for your order at {}.",
                msg.amount, msg.restaurant_name
            );
            customer_address.do_send(FinishDelivery {
                reason: reason_msg.clone(),
            });
        } else {
            self.logger
                .warn("Failed to find customer data when denying payment");
        }
        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<OrderRequest> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: OrderRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Preparing order for customer {} at restaurant {} with price {}",
            msg.customer_id, msg.restaurant_name, msg.order_price
        ));
        if let Some(customer) = self.customers.get_mut(&msg.customer_id) {
            customer.order_price = Some(msg.order_price);
            if let Some((_, peer)) = &self.next_server_peer {
                peer.do_send(UpdateCustomerData {
                    customer_id: msg.customer_id,
                    location: customer.location,
                    order_price: customer.order_price,
                })
            }
        } else {
            self.logger
                .warn("Failed to find customer data when preparing order");
        }
        self.orders_in_process.insert(
            msg.customer_id,
            OrderData {
                rider_id: None,
                order_price: Some(msg.order_price),
                customer_location: match self.customers.get(&msg.customer_id) {
                    Some(customer) => customer.location,
                    None => {
                        self.logger
                            .warn("Failed to find customer data when preparing order");
                        return;
                    }
                },
                customer_id: msg.customer_id,
            },
        );
        if let Some(restaurant) = self.restaurants.get(&msg.restaurant_name) {
            restaurant.address.do_send(OrderToRestaurant {
                customer_id: msg.customer_id,
                price: msg.order_price,
            });
        } else {
            self.logger
                .warn("Failed to find restaurant data when preparing order");
        }
        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<SendNotification> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: SendNotification, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Sending notification to customer {}: {}",
            msg.recipient_id, msg.message
        ));
        match self.customer_connections.get(&msg.recipient_id) {
            Some(customer_adress) => {
                customer_adress.do_send(PushNotification {
                    notification_msg: msg.message,
                });
            }
            None => {
                self.logger
                    .warn("Failed to find customer data when sending notification");
            }
        };
    }
}

impl Handler<OrderReady> for ConnectionManager {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: OrderReady, ctx: &mut Self::Context) -> Self::Result {
        if let Some(customer_adress) = self.customer_connections.get(&msg.customer_id) {
            let notification_msg =
                "Your order is ready! Finding a rider for you order...".to_string();
            customer_adress.do_send(PushNotification {
                notification_msg: notification_msg.to_string(),
            });
            self.logger
                .debug(&format!("Finding a rider for customer {}", msg.customer_id));
            ctx.address().do_send(FindRider {
                customer_id: msg.customer_id,
                restaurant_location: msg.restaurant_location,
            });
        } else {
            self.logger
                .warn("Failed to find customer data when notifying order ready");
        }

        Box::pin(async {}.into_actor(self))
    }
}

#[async_handler]
impl Handler<OrderCancelled> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: OrderCancelled, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(customer_adress) = self.customer_connections.get(&msg.customer_id) {
            self.orders_in_process.remove(&msg.customer_id);
            customer_adress.do_send(FinishDelivery {
                reason: "Order cancelled by the restaurant due to lack of stock".to_string(),
            });
        } else {
            self.logger
                .warn("Failed to find customer data when cancelling order");
        }
    }
}

#[async_handler]
impl Handler<FindRider> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: FindRider, _ctx: &mut Self::Context) -> Self::Result {
        let customer_loc = match self.customers.get(&msg.customer_id) {
            Some(customer) => &customer.location,
            None => {
                self.logger
                    .warn("Customer ID not found when trying to find rider");
                return;
            }
        };

        let closest = NearbyEntities::closest_riders(
            &msg.restaurant_location,
            &self.riders,
            N_RIDERS_TO_NOTIFY,
        );

        if closest.is_empty() {
            self.logger
                .warn("No riders with valid location, adding to pending requests");
            self.pending_delivery_requests.push_back(msg);
        } else {
            for addr in closest {
                addr.do_send(DeliveryOffer {
                    customer_id: msg.customer_id,
                    customer_location: *customer_loc,
                });
            }
        }
    }
}

#[async_handler]
impl Handler<DeliveryOfferAccepted> for ConnectionManager {
    type Result = ();

    async fn handle(
        &mut self,
        msg: DeliveryOfferAccepted,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        match self.customer_connections.get(&msg.customer_id) {
            Some(customer_adress) => {
                let notification_msg =
                    format!("The rider {} will deliver your order", msg.rider_id);
                customer_adress.do_send(PushNotification { notification_msg });
                if let Some(customer_data) = self.customers.get(&msg.customer_id) {
                    self.orders_in_process.insert(
                        msg.rider_id,
                        OrderData {
                            rider_id: Some(msg.rider_id),
                            order_price: customer_data.order_price,
                            customer_location: customer_data.location,
                            customer_id: msg.customer_id,
                        },
                    );
                }
            }
            None => self
                .logger
                .warn(&format!("No customer {} founr", msg.customer_id)),
        }
        match self.orders_in_process.get_mut(&msg.customer_id) {
            Some(order_data) => {
                if let Some(existing_rider_id) = order_data.rider_id {
                    // Ya hay un rider asignado, negar la oferta al rider actual
                    // Es necesario o que no les envíe nada?
                    // if let Some(rider) = self.riders.get(&msg.rider_id) {
                    //     rider.address.do_send(DeliveryOfferAlreadyAssigned {
                    //         customer_id: msg.customer_id,
                    //     });
                    self.logger.debug(&format!(
                        "Rider {} already assigned to customer {}",
                        existing_rider_id, msg.customer_id
                    ));
                } else {
                    // Asignar el rider y confirmar la oferta
                    order_data.rider_id = Some(msg.rider_id);
                    if let Some(rider) = self.riders.get(&msg.rider_id) {
                        rider.address.do_send(DeliveryOfferConfirmed {
                            customer_id: msg.customer_id,
                            customer_location: order_data.customer_location,
                        });
                    }
                    if let Some(customer_address) = self.customer_connections.get(&msg.customer_id)
                    {
                        let notification_msg =
                            format!("El rider {} entregará tu pedido", msg.rider_id);
                        customer_address.do_send(PushNotification { notification_msg });
                    }
                }
            }
            None => {
                self.logger.warn("Failed finding customer/order");
            }
        };
    }
}

#[async_handler]
impl Handler<RiderArrivedAtCustomer> for ConnectionManager {
    type Result = ();

    async fn handle(
        &mut self,
        msg: RiderArrivedAtCustomer,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        match self.orders_in_process.get(&msg.rider_id) {
            Some(OrderData { customer_id, .. }) => {
                match self.customer_connections.get(customer_id) {
                    Some(customer_adress) => {
                        let notification_msg = "The rider is outside! Pick up the order".to_owned();
                        customer_adress.do_send(PushNotification { notification_msg });
                    }

                    None => {
                        self.logger.warn("Failed finding customer");
                    }
                };
            }

            None => {
                self.logger.warn("Failed finding order in progress");
            }
        }
        self.logger.debug(&format!(
            "Rider {} arrived at customer {}",
            msg.rider_id, msg.customer_id
        ));
        if let Some(customer_adress) = self.customer_connections.get(&msg.customer_id) {
            customer_adress.do_send(PushNotification {
                notification_msg: "The rider is outside! Pick up the order".to_string(),
            });
        } else {
            self.logger
                .warn("Failed to find customer data when notifying rider arrived");
        }
    }
}

#[async_handler]
impl Handler<DeliveryDone> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: DeliveryDone, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Entering DeliveryDone handler");
        let Some(&order_data) = self.orders_in_process.get(&msg.customer_id) else {
            self.logger.warn("Failed finding order in progress");
            return;
        };

        if self.orders_in_process.remove(&msg.customer_id).is_none() {
            self.logger
                .warn("Expected order to exist for removal but it was missing");
            return;
        }

        self.logger.debug("Removed finished order");

        let price = match order_data.order_price {
            Some(p) => p,
            None => {
                self.logger
                    .warn("No se encontró el precio del pedido al ejecutar el pago");
                return;
            }
        };
        let execute_payment_msg = ExecutePayment {
            customer_id: msg.customer_id,
            price,
        };

        if let Some(payment_system) = &self.payment_system {
            payment_system.do_send(execute_payment_msg);
        } else {
            self.logger
                .warn("Failed to find payment system when finishing delivery");
        }
    }
}

#[async_handler]
impl Handler<PaymentExecuted> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: PaymentExecuted, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Payment executed for customer {} with amount {}",
            msg.customer_id, msg.amount
        ));

        if let Some(customer_adress) = self.customer_connections.get(&msg.customer_id) {
            self.orders_in_process.remove(&msg.customer_id);
            let reason_msg = format!("Payment successfully executed for order of {}.", msg.amount,);
            customer_adress.do_send(FinishDelivery {
                reason: reason_msg.clone(),
            });
        } else {
            self.logger
                .warn("Failed to find customer data when denying payment");
        }
        self.process_pending_requests();
    }
}

impl Handler<UpdateCustomerData> for ConnectionManager {
    type Result = ();

    fn handle(&mut self, msg: UpdateCustomerData, _ctx: &mut Self::Context) -> Self::Result {
        self.customers
            .entry(msg.customer_id)
            .and_modify(|data| {
                data.location = msg.location;
                data.order_price = msg.order_price
            })
            .or_insert(CustomerData {
                location: msg.location,
                order_price: msg.order_price,
            });
        if let Some(LeaderData { id: leader_id, .. }) = self.leader {
            if let Some((next_peer_id, peer)) = &self.next_server_peer {
                if !(leader_id == *next_peer_id) {
                    self.logger
                        .info(&format!("Update being sent to {next_peer_id}",));
                    peer.do_send(SendUpdateCustomerData {
                        customer_id: msg.customer_id,
                        location: msg.location,
                        order_price: msg.order_price,
                    });
                }
            }
        };

        self.logger.info(&format!(
            "Updated customer {} with {:?} location and {:?} price",
            msg.customer_id, msg.location, msg.order_price
        ))
    }
}
