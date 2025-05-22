use actix::Message;
use serde::{Deserialize, Serialize};

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct GetRestaurants;

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct Restaurants(pub String);

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum SocketMessage {
    GetRestaurants,
    Restaurants(String),
}
