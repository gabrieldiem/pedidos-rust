use crate::connection_manager::LeaderData;
use crate::connection_manager::PeerId;
use crate::{client_connection::ClientConnection, server_peer::ServerPeer};
use actix::{Addr, Message};
use common::protocol::{Location, OrderData};
use std::collections::{HashMap, VecDeque};

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct PeerDisconnected {
    pub peer_id: PeerId,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct RegisterCustomer {
    pub id: u32,
    pub address: Addr<ClientConnection>,
    pub location: Location,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct RegisterRider {
    pub id: u32,
    pub address: Addr<ClientConnection>,
    pub location: Location,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct RegisterRestaurant {
    pub name: String,
    pub id: u32,
    pub address: Addr<ClientConnection>,
    pub location: Location,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct RegisterPaymentSystem {
    pub address: Addr<ClientConnection>,
    pub port: u32,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct RegisterPeerServer {
    pub id: u32,
    pub address: Addr<ServerPeer>,
}

#[derive(Message, Debug)]
#[rtype(result = "Result<bool, ()>")]
pub struct IsPeerConnected {
    pub id: u32,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct PaymentAuthorized {
    pub customer_id: u32,
    pub amount: f64,
    pub restaurant_name: String,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct PaymentDenied {
    pub customer_id: u32,
    pub amount: f64,
    pub restaurant_name: String,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct SendRestaurantList {
    pub customer_id: u32,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct AuthorizePayment {
    pub customer_id: u32,
    pub price: f64,
    pub restaurant_name: String,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct PaymentExecuted {
    pub customer_id: u32,
    pub amount: f64,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct OrderRequest {
    pub customer_id: u32,
    pub restaurant_name: String,
    pub order_price: f64,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct OrderReady {
    pub customer_id: u32,
    pub restaurant_location: Location,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct OrderCancelled {
    pub customer_id: u32,
}

#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct FindRider {
    pub customer_id: u32,
    pub restaurant_location: Location,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct SendNotification {
    pub message: String,
    pub recipient_id: u32, // ID of the recipient aka receiver of the notification
}

#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct UpdateCustomerData {
    pub customer_id: u32,
    pub location: Location,
    pub order_price: Option<f64>,
}

#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct UpdateRestaurantData {
    pub restaurant_name: String,
    pub location: Location,
    pub pending_orders: VecDeque<OrderData>,
}

#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct UpdateRiderData {
    pub rider_id: u32,
    pub location: Option<Location>,
}

#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct UpdateOrderInProgressData {
    pub rider_id: Option<u32>,
    pub order_price: Option<f64>,
    pub customer_location: Location,
    pub customer_id: u32,
}

#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct RemoveOrderInProgressData {
    pub customer_id: u32,
}

#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct PushPendingDeliveryRequest {
    pub customer_id: u32,
    pub restaurant_location: Location,
    pub to_front: bool,
}

#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct PopPendingDeliveryRequest {}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct ElectionCallReceived {}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct ElectionCoordinatorReceived {
    pub leader_port: u32,
}

#[derive(Message, Debug)]
#[rtype(result = "Result<Option<LeaderData>, ()>")]
pub struct GetLeaderInfo {}

#[derive(Message, Debug)]
#[rtype(result = "Result<HashMap<PeerId, Addr<ServerPeer>>, ()>")]
pub struct GetPeers {}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct InitLeader {}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct GotLeaderFromPeer {
    pub leader_port: u32,
}
