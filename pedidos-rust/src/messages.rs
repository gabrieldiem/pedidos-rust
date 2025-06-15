use crate::{client_connection::ClientConnection, server_peer::ServerPeer};
use actix::{Addr, Message};
use common::protocol::Location;

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
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct RegisterPeerServer {
    pub id: u32,
    pub address: Addr<ServerPeer>,
    pub is_leader: bool,
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
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct OrderCancelled {
    pub customer_id: u32,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct FindRider {
    pub customer_id: u32,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct SendNotification {
    pub message: String,
    pub recipient_id: u32, // ID of the recipient aka receiver of the notification
}
