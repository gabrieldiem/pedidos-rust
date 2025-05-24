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
pub struct FindRider {
    pub customer_id: u32,
}
