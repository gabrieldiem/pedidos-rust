use crate::client_connection::ClientConnection;
use crate::messages::{
    AuthorizePayment, ElectionCoordinatorReceived, FindRider, GetLeaderInfo, GetPeers,
    GotLeaderFromPeer, InitLeader, IsPeerConnected, OrderCancelled, OrderReady, OrderRequest,
    PaymentAuthorized, PaymentDenied, PaymentExecuted, PeerDisconnected, PopPendingDeliveryRequest,
    PushPendingDeliveryRequest, RegisterCustomer, RegisterPaymentSystem, RegisterPeerServer,
    RegisterRestaurant, RegisterRider, RemoveOrderInProgressData, SendNotification,
    SendRestaurantList, UpdateCustomerData, UpdateOrderInProgressData, UpdateRestaurantData,
    UpdateRiderData,
};
use crate::nearby_entitys::NearbyEntities;
use crate::server_peer::ServerPeer;
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, ResponseActFuture, WrapFuture};
use actix_async_handler::async_handler;
use common::configuration::Configuration;
use common::constants::{DEFAULT_PR_HOST, N_RIDERS_TO_NOTIFY, NO_RESTAURANTS};
use common::protocol::{
    AuthorizePaymentRequest, DeliveryDone, DeliveryOffer, DeliveryOfferAccepted,
    DeliveryOfferConfirmed, ElectionCall, ElectionCoordinator, ExecutePayment, FinishDelivery,
    LeaderQuery, Location, LocationUpdateForRider, OrderToRestaurant, PushNotification,
    Restaurants, RiderArrivedAtCustomer, SendPopPendingDeliveryRequest,
    SendPushPendingDeliveryRequest, SendRemoveOrderInProgressData, SendUpdateCustomerData,
    SendUpdateOrderInProgressData, SendUpdateRestaurantData, SendUpdateRiderData, SocketMessage,
    Stop,
};
use common::utils::logger::Logger;
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;

type CustomerId = u32;
type RiderId = u32;
type RestaurantName = String;

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct NotifyAllClientsIAmLeader {}

/// `LeaderData` holds the information about the Leader of the PedidosRust instances.
///
/// Contains the ID of the leading replica and its port.
#[derive(Debug, Clone)]
pub struct LeaderData {
    pub id: u32,
    pub port: u32,
}

/// `RiderData` holds the information about a rider.
///
/// This struct is used to represent details for a rider, including their location.
/// The `location` field is optional, which allows you to represent cases where the
/// rider's location might be temporarily unavailable or unknown.
#[derive(Debug, Clone)]
pub struct RiderData {
    /// The current location of the rider.
    ///
    /// This field uses an `Option` to indicate that a location may or may not be present.
    /// When set to `Some(location)`, it represents the rider's coordinate via a `Location` struct.
    /// When set to `None`, it indicates that the location is unknown. This usually happens when the
    /// rider has been created but has not connected entirely with the manager.
    pub location: Option<Location>,
}

/// `OrderData` holds the information about current, unfinished orders.
///
/// Contains the ID of the Rider and of the Customer, the order price if it has been
/// set, and the location of the Customer
#[derive(Debug, Clone, Copy)]
pub struct OrderData {
    pub rider_id: Option<RiderId>,
    pub order_price: Option<f64>,
    pub customer_location: Location,
    pub customer_id: CustomerId,
}

/// `CustomerData` holds the information about customers waiting for an order to be
/// delivered.
///
/// Contains the location of the customer and the order price, if it has been set
#[derive(Debug, Clone)]
pub struct CustomerData {
    pub location: Location,
    pub order_price: Option<f64>,
}

/// `CustomerData` holds the information about customers waiting for an order to be
/// delivered.
///
/// Contains the location of the customer and the order price, if it has been set
impl CustomerData {
    pub fn new(location: Location, order_price: Option<f64>) -> CustomerData {
        CustomerData {
            location,
            order_price,
        }
    }
}

/// `RestaurantData` holds the information about restaurants
///
/// Contains the location of the restaurant.
#[derive(Debug, Clone)]
pub struct RestaurantData {
    pub location: Location,
}

impl RestaurantData {
    pub fn new(location: Location) -> RestaurantData {
        RestaurantData { location }
    }
}

pub type PeerId = u32;

/// Manages all the client connections and inter-server communications, and holds
/// the current data of the system
///
/// The `ConnectionManager` is responsible for:
/// * Maintaining connections with customers, riders, restaurants, and payment systems.
/// * Handling orders in process and pending delivery requests.
/// * Managing peer servers for a distributed system, including leader election.
///
/// # Fields
///
/// * `logger` - A logger instance used for logging debug, info, and warning messages.
/// * `id` - The unique identifier for the connection manager.
/// * `port` - The port on which the connection manager listens.
/// * `configuration` - Application configuration parameters relevant to the connection manager.
///
/// ## Entities
///
/// * `customer_connections` - A mapping of customer IDs to their associated client connection actor addresses.
/// * `customers` - A mapping of customer IDs to `CustomerData`.
/// * `rider_connections` - A mapping of rider IDs to their associated client connection actor addresses.
/// * `riders` - A mapping of rider IDs to `RiderData`.
/// * `restaurant_connections` - A mapping of restaurant names to their associated client connection actor addresses.
/// * `restaurants` - A mapping of restaurant names to `RestaurantData`.
/// * `payment_system` - An optional address for the payment system client connection.
///
/// ## Orders and Requests
///
/// * `orders_in_process` - A mapping of customer IDs to `OrderData` for orders currently in process.
/// * `pending_delivery_requests` - A queue of pending delivery requests to be processed.
///
/// ## Server Peers
///
/// * `next_server_peer` - Information about the next peer server in the network (if any). Used for data transfers
///   between replicas of the PedidosRust application.
/// * `server_peers` - A mapping of peer IDs to their associated server peer actor addresses.
/// * `leader` - Information about the current leader server (if any).
/// * `election_in_progress` - A flag indicating whether a leader election is currently underway.
pub struct ConnectionManager {
    pub logger: Logger,
    pub id: u32,
    pub port: u32,
    pub configuration: Configuration,
    pub udp_socket: Arc<UdpSocket>,

    // Entities
    pub customer_connections: HashMap<CustomerId, Addr<ClientConnection>>,
    pub customers: HashMap<CustomerId, CustomerData>,
    pub rider_connections: HashMap<RiderId, Addr<ClientConnection>>,
    pub riders: HashMap<RiderId, RiderData>,
    pub restaurant_connections: HashMap<RestaurantName, Addr<ClientConnection>>,
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
    pub fn new(
        id: u32,
        port: u32,
        configuration: Configuration,
        udp_socket: Arc<UdpSocket>,
    ) -> ConnectionManager {
        ConnectionManager {
            id,
            port,
            configuration,
            logger: Logger::new(Some("[CONNECTION-MANAGER]")),
            rider_connections: HashMap::new(),
            riders: HashMap::new(),
            customer_connections: HashMap::new(),
            customers: HashMap::new(),
            orders_in_process: HashMap::new(),
            pending_delivery_requests: VecDeque::new(),
            restaurant_connections: HashMap::new(),
            restaurants: HashMap::new(),
            payment_system: None,
            server_peers: HashMap::new(),
            leader: None,
            next_server_peer: None,
            election_in_progress: false,
            udp_socket,
        }
    }

    /// Processes pending delivery requests.
    ///
    /// This method iterates through the `pending_delivery_requests` queue and:
    ///
    /// 1. If a `next_server_peer` exists, forwards a notification using `SendPopPendingDeliveryRequest`.
    /// 2. Finds a registered rider connection to assign the delivery request.
    /// 3. If a corresponding customer exists, sends a `DeliveryOffer` message to the rider.
    /// 4. If no rider is available, requeues the delivery request and notifies the peer via
    ///    `SendPushPendingDeliveryRequest`.
    ///
    /// The method stops processing as soon as it cannot assign a pending request, ensuring that
    /// no delivery request is lost.
    fn process_pending_requests(&mut self) {
        while let Some(pending_request) = self.pending_delivery_requests.pop_front() {
            if let Some((_, peer)) = &self.next_server_peer {
                peer.do_send(SendPopPendingDeliveryRequest {});
            }
            if let Some(rider_adress) = self.rider_connections.values().find(|_rider| true) {
                self.logger
                    .debug("Assigned pending delivery request to newly registered rider");
                match self.customers.get(&pending_request.customer_id) {
                    Some(customer) => {
                        rider_adress.do_send(DeliveryOffer {
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
                self.pending_delivery_requests
                    .push_front(pending_request.clone());
                if let Some((_, peer)) = &self.next_server_peer {
                    peer.do_send(SendPushPendingDeliveryRequest {
                        customer_id: pending_request.customer_id,
                        restaurant_location: pending_request.restaurant_location,
                        to_front: true,
                    });
                }
                break;
            }
        }
    }

    /// Sends a message to the next server peer, if one is available.
    ///
    /// This method logs the action and forwards the provided message to the next peer.
    ///
    /// # Type Parameters
    ///
    /// * `M` - The type of the message to send. This must be:
    ///   - `'static` so that it lives long enough for the actor system.
    ///   - `Send` since it may be sent between threads.
    ///   - Have a result that is also `Send`.
    ///   - Handled by the `ServerPeer` actor.
    ///
    /// # Arguments
    ///
    /// * `msg` - The message to send to the next server peer.
    fn send_message_to_next_peer<M>(&self, msg: M)
    where
        M: actix::Message + Send + 'static,
        <M as actix::Message>::Result: Send,
        ServerPeer: Handler<M>,
    {
        if let Some(LeaderData { id: leader_id, .. }) = self.leader {
            if let Some((next_peer_id, peer)) = &self.next_server_peer {
                if leader_id != *next_peer_id {
                    self.logger
                        .info(&format!("Update being sent to {next_peer_id}",));
                    peer.do_send(msg);
                } else {
                    self.logger
                        .info("No next server peer available to send the message");
                }
            }
        };
    }

    fn update_next_peer(&mut self) {
        if self.server_peers.is_empty() {
            self.logger
                .debug("No next server peer because there are no peers");
            self.next_server_peer = None;
            return;
        }

        let next_peer_id = self
            .server_peers
            .keys()
            .filter(|&&id| id > self.id)
            .min()
            .copied()
            .or_else(|| self.server_peers.keys().min().copied());

        // Update the next server peer if found.
        self.next_server_peer = next_peer_id.and_then(|peer_id| {
            self.server_peers
                .get(&peer_id)
                .cloned()
                .map(|addr| (peer_id, addr))
        });

        if let Some(next_peer_id) = self.next_server_peer.clone() {
            self.logger.info(&format!(
                "Updated next peer server to peer with id {:?}",
                next_peer_id.0
            ));
        }
    }

    fn serialize_message(message: SocketMessage) -> Result<String, Box<dyn std::error::Error>> {
        let msg_to_send = serde_json::to_string(&message)?;
        Ok(msg_to_send)
    }

    async fn notify_every_client_i_am_new_leader(
        client_ports: Vec<u32>,
        udp_socket: Arc<UdpSocket>,
        logger: Logger,
        leader_port: u32,
        leader_id: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for client_port in client_ports {
            logger.debug(&format!(
                "Informing client with port {client_port} that I am new leader with ID {leader_id}"
            ));

            let client_addr: SocketAddr =
                match format!("{}:{}", DEFAULT_PR_HOST, client_port).parse() {
                    Ok(addr) => addr,
                    Err(e) => {
                        logger.error(&format!(
                            "Failed to parse client address for port: {client_port}. {e}"
                        ));
                        continue;
                    }
                };

            let msg: String = match Self::serialize_message(SocketMessage::ReconnectionMandate(
                leader_id,
                leader_port,
            )) {
                Ok(msg) => msg,
                Err(e) => {
                    logger.error(&format!("Failed to serialize message: {e}"));
                    continue;
                }
            };

            match udp_socket.send_to(msg.as_bytes(), client_addr).await {
                Ok(_) => {}
                Err(e) => {
                    logger.error(&format!("Failed to send UDP message: {e}"));
                    continue;
                }
            };
        }

        Ok(())
    }
}

impl Actor for ConnectionManager {
    type Context = Context<Self>;
}

#[async_handler]
impl Handler<LeaderQuery> for ConnectionManager {
    type Result = ();

    /// Broadcasts a `LeaderQuery` message to all registered server peers.
    ///
    /// # Arguments
    ///
    /// * `_msg` - A `LeaderQuery` message (unused in this handler).
    /// * `_ctx` - The actor context (unused).
    async fn handle(&mut self, _msg: LeaderQuery, _ctx: &mut Self::Context) -> Self::Result {
        for peer_id in self.server_peers.keys() {
            match self.server_peers.get(peer_id) {
                Some(peer_addr) => {
                    peer_addr.do_send(LeaderQuery {});
                }
                None => {
                    self.logger.warn("No peer found");
                }
            };
        }
    }
}

#[async_handler]
impl Handler<InitLeader> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, _msg: InitLeader, _ctx: &mut Self::Context) -> Self::Result {
        if self.server_peers.is_empty() {
            self.logger.info(&format!(
                "No peers present. Taking the leader role with ID {}",
                self.id
            ));

            self.leader = Some(LeaderData {
                id: self.id,
                port: self.port,
            });
        }
    }
}

#[async_handler]
impl Handler<PeerDisconnected> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: PeerDisconnected, _ctx: &mut Self::Context) -> Self::Result {
        let peer_id = msg.peer_id;
        self.logger
            .warn(&format!("Peer with id {peer_id} disconnected"));
        let mut was_the_disconnected_peer_the_leader = false;

        if let Some(leader_data) = self.leader.clone() {
            if leader_data.id == peer_id {
                was_the_disconnected_peer_the_leader = true;
            }
        }

        match self.server_peers.remove(&peer_id) {
            Some(peer_addr) => {
                peer_addr.do_send(Stop {});
                self.logger
                    .debug(&format!("Removed peer with id {peer_id}"));
            }
            None => {
                self.logger
                    .debug(&format!("No peer connection with id {peer_id}"));
            }
        }

        if was_the_disconnected_peer_the_leader && self.server_peers.is_empty() {
            // There is no one so I will be the leader
            _ctx.address().do_send(InitLeader {});
            _ctx.address().do_send(NotifyAllClientsIAmLeader {});
        } else if was_the_disconnected_peer_the_leader {
            // There is at least one peer still so I call elections
            _ctx.address().do_send(ElectionCall {});
        }

        self.update_next_peer();
    }
}

#[async_handler]
impl Handler<GetPeers> for ConnectionManager {
    type Result = Result<HashMap<PeerId, Addr<ServerPeer>>, ()>;

    async fn handle(&mut self, _msg: GetPeers, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.server_peers.clone())
    }
}

#[async_handler]
impl Handler<GotLeaderFromPeer> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: GotLeaderFromPeer, _ctx: &mut Self::Context) -> Self::Result {
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
                    "Found leader from peer. Leader is {} with ID {}",
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
impl Handler<ElectionCoordinatorReceived> for ConnectionManager {
    type Result = ();

    /// Handles an `ElectionCoordinatorReceived` message.
    ///
    /// This async handler resets the election flag, looks up the leader by port in the
    /// configuration and updates the leader information if found
    ///
    /// # Arguments
    ///
    /// * `msg` - The message carrying the leader's port.
    /// * `_ctx` - The actor context (unused in this handler).
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

    /// Initiates a leader election by handling an `ElectionCall` message.
    ///
    /// If no election is in progress, this handler sends an `ElectionCall` to all peers
    /// with an ID greater than the current node's. If at least one such peer exists,
    /// it flags that an election is underway. If no higher-ID peer exists, the current
    /// node assumes leadership and sends an `ElectionCoordinator` message to all peers.
    ///
    /// # Arguments
    ///
    /// * `_msg` - The incoming `ElectionCall` message (content unused).
    /// * `_ctx` - The actor context (unused).
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
            _ctx.address().do_send(NotifyAllClientsIAmLeader {});
        }
    }
}

impl Handler<NotifyAllClientsIAmLeader> for ConnectionManager {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        _msg: NotifyAllClientsIAmLeader,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.logger
            .info("Notifying all clients I am the new leader");

        let mut customer_ports: Vec<u32> = Vec::new();
        for customer_id in self.customers.keys() {
            customer_ports.push(*customer_id);
        }

        if customer_ports.is_empty() {
            self.logger.debug("No Customers to notify");
        }

        let mut rider_ports: Vec<u32> = Vec::new();
        for rider_id in self.riders.keys() {
            rider_ports.push(*rider_id);
        }

        if rider_ports.is_empty() {
            self.logger.debug("No Riders to notify");
        }

        let mut restaurant_ports: Vec<u32> = Vec::new();
        for restuarant_name in self.restaurants.keys() {
            for restaurant_info in self.configuration.restaurant.infos.clone() {
                if *restuarant_name == restaurant_info.name {
                    restaurant_ports.push(restaurant_info.port);
                }
            }
        }

        if restaurant_ports.is_empty() {
            self.logger.debug("No Restaurants to notify");
        }

        let udp_socket = self.udp_socket.clone();
        let logger = self.logger.clone();
        let my_port = self.port;
        let my_id = self.id;

        Box::pin(
            async move {
                let _ = Self::notify_every_client_i_am_new_leader(
                    customer_ports,
                    udp_socket.clone(),
                    logger.clone(),
                    my_port,
                    my_id,
                )
                .await;
                let _ = Self::notify_every_client_i_am_new_leader(
                    rider_ports,
                    udp_socket.clone(),
                    logger.clone(),
                    my_port,
                    my_id,
                )
                .await;
                let _ = Self::notify_every_client_i_am_new_leader(
                    restaurant_ports,
                    udp_socket.clone(),
                    logger.clone(),
                    my_port,
                    my_id,
                )
                .await;
            }
            .into_actor(self),
        )
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

    /// Registers a new peer server and updates the next server peer.
    ///
    /// This handler:
    /// - Inserts the provided peer address into `server_peers` if it is not already present.
    /// - Updates `next_server_peer` and logs the updated peer.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `RegisterPeerServer` message containing the peer's ID and address.
    /// * `_ctx` - The actor context (unused in this handler).
    async fn handle(&mut self, msg: RegisterPeerServer, _ctx: &mut Self::Context) -> Self::Result {
        let peer_address = msg.address;
        self.logger
            .debug(&format!("Registering server peer with ID {}", msg.id));
        self.server_peers
            .entry(msg.id)
            .or_insert(peer_address.clone());

        self.update_next_peer();
        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<IsPeerConnected> for ConnectionManager {
    type Result = Result<bool, ()>;

    /// Checks whether a peer is connected.
    ///
    /// This handler inspects the server peers to determine if the peer with the given ID
    /// is connected.
    ///
    /// # Arguments
    ///
    /// * `msg` - The `IsPeerConnected` message containing the peer ID to check.
    /// * `_ctx` - The actor context (unused).
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if the peer is connected.
    /// * `Ok(false)` if the peer is not connected.
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

    /// Registers a new rider.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `RegisterRider` message containing the rider's ID, address, and location.
    /// * `_ctx` - The actor context (unused).
    async fn handle(&mut self, msg: RegisterRider, _ctx: &mut Self::Context) -> Self::Result {
        self.logger
            .debug(&format!("Registering Rider with ID {}", msg.id));
        self.rider_connections.insert(msg.id, msg.address);
        self.riders.entry(msg.id).or_insert(RiderData {
            location: Some(msg.location),
        });
        self.send_message_to_next_peer(SendUpdateRiderData {
            rider_id: msg.id,
            location: Some(msg.location),
        });
        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<RegisterCustomer> for ConnectionManager {
    type Result = ();

    /// Registers a new customer.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `RegisterCustomer` message carrying the customer's ID, address, and location.
    /// * `_ctx` - The actor context (unused).
    async fn handle(&mut self, msg: RegisterCustomer, _ctx: &mut Self::Context) -> Self::Result {
        self.logger
            .debug(&format!("Registering Customer with ID {}", msg.id));
        self.customers
            .entry(msg.id)
            .or_insert(CustomerData::new(msg.location, None));
        self.customer_connections
            .insert(msg.id, msg.address.clone());
        self.send_message_to_next_peer(SendUpdateCustomerData {
            customer_id: msg.id,
            location: msg.location,
            order_price: None,
        });
        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<RegisterRestaurant> for ConnectionManager {
    type Result = ();

    /// Registers a new restaurant.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `RegisterRestaurant` message containing the restaurant's ID, name, address, and location.
    /// * `_ctx` - The actor context (unused).
    async fn handle(&mut self, msg: RegisterRestaurant, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Registering Restaurant with ID {} and name {}",
            msg.id, msg.name
        ));
        self.restaurants
            .entry(msg.name.clone())
            .or_insert(RestaurantData::new(msg.location));
        self.restaurant_connections
            .insert(msg.name.clone(), msg.address);

        self.send_message_to_next_peer(SendUpdateRestaurantData {
            restaurant_name: msg.name,
            location: msg.location,
        });

        self.process_pending_requests();
    }
}

#[async_handler]
impl Handler<RegisterPaymentSystem> for ConnectionManager {
    type Result = ();

    /// Registers the payment system.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `RegisterPaymentSystem` message containing the payment system's address.
    /// * `_ctx` - The actor context (unused).
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

    /// Sends nearby restaurants to a customer.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `SendRestaurantList` message with the customer's ID.
    /// * `_ctx` - The actor context (unused).
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

    /// Authorizes a payment for a customer.
    ///
    /// This handler constructs an `AuthorizePaymentRequest` from the incoming message and,
    /// if a payment system is available, forwards the request to it. Otherwise, it notifies the
    /// customer by sending a `FinishDelivery` message with a failure reason.
    ///
    /// # Arguments
    ///
    /// * `msg` - The `AuthorizePayment` message containing customer ID, price, and restaurant name.
    /// * `_ctx` - The actor context (unused).
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

    /// Authorizes a payment for a customer.
    ///
    /// This handler constructs an `AuthorizePaymentRequest` from the incoming message and,
    /// if a payment system is available, forwards the request to it. Otherwise, it  notifies
    /// the customer by sending a `FinishDelivery` message with a failure reason.
    ///
    /// # Arguments
    ///
    /// * `msg` - The `AuthorizePayment` message containing customer ID, price, and restaurant name.
    /// * `_ctx` - The actor context (unused).
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

    /// Handles a denied payment.
    ///
    /// This handler notifies the customer by sending a
    /// `FinishDelivery` message with the denial reason.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `PaymentDenied` message containing the customer's ID, amount, and restaurant name.
    /// * `_ctx` - The actor context (unused).
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

    /// Prepares and processes a customer's order request.
    ///
    /// This handler performs multiple steps:
    ///
    /// 1. Updates the customer's order price in `customers` and notifies the next peer with updated data.
    /// 2. Inserts the order into `orders_in_process` with the customer's location. Broadcasts the update
    /// 3. Forwards the order to the corresponding restaurant.
    ///
    /// # Arguments
    ///
    /// * `msg` - An `OrderRequest` message containing the customer ID, restaurant name, and order price.
    /// * `_ctx` - The actor context (unused).
    async fn handle(&mut self, msg: OrderRequest, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Preparing order for customer {} at restaurant {} with price {}",
            msg.customer_id, msg.restaurant_name, msg.order_price
        ));
        if let Some(customer) = self.customers.get_mut(&msg.customer_id) {
            customer.order_price = Some(msg.order_price);
            let location = customer.location;
            let order_price = customer.order_price;
            // The mutable borrow ends here.
            self.send_message_to_next_peer(SendUpdateCustomerData {
                customer_id: msg.customer_id,
                location,
                order_price,
            });
        } else {
            self.logger
                .warn("Failed to find customer data when preparing order");
        }
        let customer_location = match self.customers.get(&msg.customer_id) {
            Some(customer) => customer.location,
            None => {
                self.logger
                    .warn("Failed to find customer data when preparing order");
                return;
            }
        };
        self.orders_in_process.insert(
            msg.customer_id,
            OrderData {
                rider_id: None,
                order_price: Some(msg.order_price),
                customer_location,
                customer_id: msg.customer_id,
            },
        );

        self.send_message_to_next_peer(SendUpdateOrderInProgressData {
            customer_id: msg.customer_id,
            customer_location,
            order_price: Some(msg.order_price),
            rider_id: None,
        });

        if let Some(restaurant_address) = self.restaurant_connections.get(&msg.restaurant_name) {
            restaurant_address.do_send(OrderToRestaurant {
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

    /// Sends a push notification to a customer.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `SendNotification` message containing the recipient's ID and the notification message.
    /// * `_ctx` - The actor context (unused).
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

    /// Notifies the customer that the order is ready and initiates a rider search.
    ///
    /// This handler sends a push notification to the customer informing them that their order is ready,
    /// triggers a `FindRider` message to begin searching for a rider and using the restaurant location
    /// from the message
    ///
    /// # Arguments
    ///
    /// * `msg` - An `OrderReady` message with the customer's ID and restaurant location.
    /// * `ctx` - The actor context, used to dispatch further messages (e.g., `FindRider`).
    ///
    /// # Returns
    ///
    /// A pinned actor future.
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

    /// Processes an order cancellation.
    ///
    /// This handler checks if the customer is connected. If so, it removes the order from the in-progress list
    ///
    /// # Arguments
    ///
    /// * `msg` - An `OrderCancelled` message containing the customer's ID.
    /// * `_ctx` - The actor context (unused).
    async fn handle(&mut self, msg: OrderCancelled, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(customer_adress) = self.customer_connections.get(&msg.customer_id) {
            self.orders_in_process.remove(&msg.customer_id);
            customer_adress.do_send(FinishDelivery {
                reason: "Order cancelled by the restaurant due to lack of stock".to_string(),
            });
            self.send_message_to_next_peer(SendRemoveOrderInProgressData {
                customer_id: msg.customer_id,
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

    /// Finds the closest available riders for a customer's order.
    ///
    /// This handler retrieves the customer's location, determines the closest riders based
    /// on the restaurant's location using `NearbyEntities::closest_riders`, if no suitable riders
    /// are found adds the request to pending requests, and forwards the pending request. Otherwise,
    /// sends a `DeliveryOffer` to each of the closest riders.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `FindRider` message containing the customer's ID and restaurant location.
    /// * `_ctx` - The actor context (unused).
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
            &self.rider_connections,
            N_RIDERS_TO_NOTIFY,
        );

        if closest.is_empty() {
            self.logger
                .warn("No riders with valid location, adding to pending requests");
            self.pending_delivery_requests.push_back(msg.clone());
            self.send_message_to_next_peer(SendPushPendingDeliveryRequest {
                customer_id: msg.customer_id,
                restaurant_location: msg.restaurant_location,
                to_front: false,
            });
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

    /// Processes a customer's acceptance of a delivery offer.
    ///
    /// This handler:
    /// - Notifies the customer that the selected rider will deliver their order and updates
    /// the rider data on the custome
    ///
    /// # Arguments
    ///
    /// * `msg` - A `DeliveryOfferAccepted` message containing the customer ID and rider ID.
    /// * `_ctx` - The actor context (unused).
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
                    self.send_message_to_next_peer(SendUpdateOrderInProgressData {
                        customer_id: msg.customer_id,
                        customer_location: customer_data.location,
                        order_price: customer_data.order_price,
                        rider_id: Some(msg.rider_id),
                    });
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
                    if let Some(rider_address) = self.rider_connections.get(&msg.rider_id) {
                        rider_address.do_send(DeliveryOfferConfirmed {
                            customer_id: msg.customer_id,
                            customer_location: order_data.customer_location,
                        });
                    }
                    if let Some((next_peer_id, peer)) = &self.next_server_peer {
                        self.logger
                            .info(&format!("Update being sent to {next_peer_id}",));
                        peer.do_send(SendUpdateOrderInProgressData {
                            customer_id: msg.customer_id,
                            customer_location: order_data.customer_location,
                            order_price: order_data.order_price,
                            rider_id: Some(msg.rider_id),
                        });
                    };
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

    /// Processes a customer's acceptance of a delivery offer.
    ///
    /// This handler notifies the customer that the selected rider will deliver their order.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `DeliveryOfferAccepted` message containing the customer ID and rider ID.
    /// * `_ctx` - The actor context (unused).
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
impl Handler<LocationUpdateForRider> for ConnectionManager {
    type Result = ();

    /// Updates the location for a rider and propagates the change to the next server peer.
    ///
    /// This handler logs the received location update, attempts to update the rider's location
    /// in the local `riders` state, and sends a `SendUpdateRiderData` message to the next peer
    /// to notify of the change.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `LocationUpdateForRider` message that includes the rider's ID and new location.
    /// * `_ctx` - The actor context (unused).
    async fn handle(
        &mut self,
        msg: LocationUpdateForRider,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.logger.debug(&format!(
            "Updating location for rider {} to {:?}",
            msg.rider_id, msg.new_location
        ));

        if let Some(rider_data) = self.riders.get_mut(&msg.rider_id) {
            rider_data.location = Some(msg.new_location);
        } else {
            self.logger.warn(&format!(
                "Tried to update location for unknown rider: {}",
                msg.rider_id
            ));
        }
        self.send_message_to_next_peer(SendUpdateRiderData {
            rider_id: msg.rider_id,
            location: Some(msg.new_location),
        });
    }
}

#[async_handler]
impl Handler<DeliveryDone> for ConnectionManager {
    type Result = ();

    /// Finalizes a delivery and triggers payment execution.
    ///
    /// This handler performs several steps:
    ///
    /// 1. Attempts to retrieve the order data for the specified customer.
    /// 2. Removes the order from the orders in process and notifies the next server peer to
    /// remove the order.
    /// 3. Constructs and sends `ExecutePayment` message using the customer ID and order price.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `DeliveryDone` message containing the customer ID whose delivery is completed.
    /// * `_ctx` - The actor context (unused).
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

        self.send_message_to_next_peer(SendRemoveOrderInProgressData {
            customer_id: msg.customer_id,
        });

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

    /// Finalizes the order after a successful payment execution.
    ///
    /// This handler:
    /// - Removes the order from orders in process and notifies the next peer to remove the order.
    /// - Sends a finish delivery message to the customer indicating the payment was successful.
    /// - Processes any pending requests.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `PaymentExecuted` message containing the customer ID and the executed payment amount.
    /// * `_ctx` - The actor context (unused).
    async fn handle(&mut self, msg: PaymentExecuted, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Payment executed for customer {} with amount {}",
            msg.customer_id, msg.amount
        ));

        if let Some(customer_adress) = self.customer_connections.get(&msg.customer_id) {
            self.orders_in_process.remove(&msg.customer_id);
            self.send_message_to_next_peer(SendRemoveOrderInProgressData {
                customer_id: msg.customer_id,
            });
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

    /// Updates the data for a customer and propagates the change.
    ///
    /// This handler performs the following actions:
    ///
    /// 1. Updates the customer's location and order price if the customer data already exists;
    ///    otherwise, inserts a new entry with the provided data.
    /// 2. Propagates the update to the next peer by sending a `SendUpdateCustomerData` message.
    ///
    /// # Arguments
    ///
    /// * `msg` - An `UpdateCustomerData` message containing the customer's ID, the new location,
    ///   and the order price.
    /// * `_ctx` - The actor context (unused in this handler).
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
        self.send_message_to_next_peer(SendUpdateCustomerData {
            customer_id: msg.customer_id,
            location: msg.location,
            order_price: msg.order_price,
        });

        self.logger.info(&format!(
            "Updated customer {} with {:?} location and {:?} price",
            msg.customer_id, msg.location, msg.order_price
        ));

        self.process_pending_requests();
    }
}

impl Handler<UpdateRestaurantData> for ConnectionManager {
    type Result = ();

    /// Updates the data for a restaurant and forwards the update.
    ///
    /// This handler performs the following actions:
    ///
    /// 1. Updates the restaurant’s location if the restaurant data already exists; otherwise,
    ///    it creates a new entry with the provided location.
    /// 2. Propagates the update to the next server peer by sending a `SendUpdateRestaurantData` message.
    ///
    /// # Arguments
    ///
    /// * `msg` - An `UpdateRestaurantData` message containing the restaurant's name and new location.
    /// * `_ctx` - The actor context (unused in this handler).
    fn handle(&mut self, msg: UpdateRestaurantData, _ctx: &mut Self::Context) -> Self::Result {
        self.restaurants
            .entry(msg.restaurant_name.clone())
            .and_modify(|data| {
                data.location = msg.location;
            })
            .or_insert(RestaurantData {
                location: msg.location,
            });
        self.send_message_to_next_peer(SendUpdateRestaurantData {
            restaurant_name: msg.restaurant_name.clone(),
            location: msg.location,
        });

        self.logger.info(&format!(
            "Updated restaurant {} with {:?} location",
            msg.restaurant_name, msg.location
        ));

        self.process_pending_requests();
    }
}

impl Handler<UpdateRiderData> for ConnectionManager {
    type Result = ();

    /// Updates the location data for a rider and propagates the change.
    ///
    /// This handler performs the following actions:
    ///
    /// 1. Updates the rider’s location if the rider data already exists; otherwise, it creates a new
    ///    entry with the provided location.
    /// 2. Sends a `SendUpdateRiderData` message to the next server peer to propagate the update.
    ///
    /// # Arguments
    ///
    /// * `msg` - An `UpdateRiderData` message containing the rider's ID and the new location.
    /// * `_ctx` - The actor context (unused).
    fn handle(&mut self, msg: UpdateRiderData, _ctx: &mut Self::Context) -> Self::Result {
        self.riders
            .entry(msg.rider_id)
            .and_modify(|data| {
                data.location = msg.location;
            })
            .or_insert(RiderData {
                location: msg.location,
            });
        self.send_message_to_next_peer(SendUpdateRiderData {
            rider_id: msg.rider_id,
            location: msg.location,
        });

        self.logger.info(&format!(
            "Updated rider data {} to location {:?}",
            msg.rider_id, msg.location
        ));

        self.process_pending_requests();
    }
}

impl Handler<UpdateOrderInProgressData> for ConnectionManager {
    type Result = ();

    /// Updates the details of an order in progress and propagates the update.
    ///
    /// This handler:
    ///
    /// 1. Updates the order information (customer location, order price, and assigned rider)
    ///    in the local `orders_in_process` map. If an entry does not exist, it creates one.
    /// 2. Propagates the update to the next server peer by sending a `SendUpdateOrderInProgressData` message.
    ///
    /// # Arguments
    ///
    /// * `msg` - An `UpdateOrderInProgressData` message containing the customer's ID,
    ///   customer location, order price, and optionally a rider ID.
    /// * `_ctx` - The actor context (unused in this handler).
    fn handle(&mut self, msg: UpdateOrderInProgressData, _ctx: &mut Self::Context) -> Self::Result {
        self.orders_in_process
            .entry(msg.customer_id)
            .and_modify(|data| {
                data.customer_id = msg.customer_id;
                data.customer_location = msg.customer_location;
                data.order_price = msg.order_price;
                data.rider_id = msg.rider_id
            })
            .or_insert(OrderData {
                customer_id: msg.customer_id,
                customer_location: msg.customer_location,
                order_price: msg.order_price,
                rider_id: msg.rider_id,
            });
        self.send_message_to_next_peer(SendUpdateOrderInProgressData {
            customer_id: msg.customer_id,
            customer_location: msg.customer_location,
            order_price: msg.order_price,
            rider_id: msg.rider_id,
        });

        self.logger.info(&format!(
            "Updated order in progress for customer {}, location {:?}, price {:?}, rider {:?}",
            msg.customer_id, msg.customer_location, msg.order_price, msg.rider_id
        ));

        self.process_pending_requests();
    }
}

impl Handler<RemoveOrderInProgressData> for ConnectionManager {
    type Result = ();

    /// Removes an order in progress and propagates the removal.
    ///
    /// This handler performs the following actions:
    ///
    /// 1. Removes the order associated with the given customer ID from the local `orders_in_process` map.
    /// 2. Notifies the next server peer of the removal by sending a `SendRemoveOrderInProgressData` message.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `RemoveOrderInProgressData` message containing the customer ID of the order to remove.
    /// * `_ctx` - The actor context (unused in this handler).
    fn handle(&mut self, msg: RemoveOrderInProgressData, _ctx: &mut Self::Context) -> Self::Result {
        self.orders_in_process.remove(&msg.customer_id);
        self.send_message_to_next_peer(SendRemoveOrderInProgressData {
            customer_id: msg.customer_id,
        });

        self.logger.info(&format!(
            "Removed order in progress for customer {}",
            msg.customer_id
        ));

        self.process_pending_requests();
    }
}

impl Handler<PushPendingDeliveryRequest> for ConnectionManager {
    type Result = ();

    /// Adds a pending delivery request to the queue and propagates the change.
    ///
    /// This handler performs the following actions:
    ///
    /// 1. Wraps the customer ID and restaurant location from the incoming message into a `FindRider` structure.
    /// 2. Inserts the new request into the `pending_delivery_requests` queue:
    ///    - At the front if `msg.to_front` is true.
    ///    - At the back otherwise.
    /// 3. Propagates the pending delivery request to the next server peer by sending a
    ///    `SendPushPendingDeliveryRequest` message.
    ///
    /// # Arguments
    ///
    /// * `msg` - A `PushPendingDeliveryRequest` message containing:
    ///     - `customer_id`: the ID of the customer.
    ///     - `restaurant_location`: the location of the restaurant.
    ///     - `to_front`: a boolean indicating whether to insert the request at the front of the queue.
    /// * `_ctx` - The actor context (unused in this handler).
    fn handle(
        &mut self,
        msg: PushPendingDeliveryRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if msg.to_front {
            self.pending_delivery_requests.push_front(FindRider {
                customer_id: msg.customer_id,
                restaurant_location: msg.restaurant_location,
            });
        } else {
            self.pending_delivery_requests.push_back(FindRider {
                customer_id: msg.customer_id,
                restaurant_location: msg.restaurant_location,
            });
        }
        self.send_message_to_next_peer(SendPushPendingDeliveryRequest {
            customer_id: msg.customer_id,
            restaurant_location: msg.restaurant_location,
            to_front: msg.to_front,
        });

        self.logger.info(&format!(
            "Pushed new pending order for customer {} and restaurant location {:?}",
            msg.customer_id, msg.restaurant_location
        ));

        self.process_pending_requests();
    }
}

impl Handler<PopPendingDeliveryRequest> for ConnectionManager {
    type Result = ();

    /// Removes a pending delivery request from the front of the queue and propagates the update.
    ///
    /// This handler performs the following actions:
    ///
    /// 1. Pops (removes) the pending delivery request from the front of the `pending_delivery_requests` queue.
    /// 2. Notifies the next server peer by sending a `SendPopPendingDeliveryRequest` message.
    ///
    /// # Arguments
    ///
    /// * `_msg` - A `PopPendingDeliveryRequest` message (unused in this handler).
    /// * `_ctx` - The actor context (unused in this handler).
    fn handle(
        &mut self,
        _msg: PopPendingDeliveryRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let popped = self.pending_delivery_requests.pop_front();
        self.send_message_to_next_peer(SendPopPendingDeliveryRequest {});

        self.logger.debug(&format!("Popped request {:?}", popped));

        self.process_pending_requests();
    }
}
