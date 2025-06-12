use crate::client_connection::ClientConnection;
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
pub struct SendRestaurantList {
    pub customer_id: u32,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct PrepareOrder {
    pub customer_id: u32,
    pub restaurant_name: String,
    pub order_price: f64,
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
