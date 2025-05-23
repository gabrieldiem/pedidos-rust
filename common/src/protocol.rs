use actix::Message;
use serde::{Deserialize, Serialize};

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct GetRestaurants;

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct Restaurants(pub String);

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct Order(pub OrderContent);

#[derive(Serialize, Deserialize, Debug)]
pub struct OrderContent {
    pub restaurant: String,
    pub amount: f64,
}

impl OrderContent {
    pub fn new(restaurant: String, amount: f64) -> OrderContent {
        OrderContent { restaurant, amount }
    }
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct PushNotification(pub String);

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum SocketMessage {
    GetRestaurants,
    Restaurants(String),
    Order(OrderContent),
    PushNotification(String),
}
