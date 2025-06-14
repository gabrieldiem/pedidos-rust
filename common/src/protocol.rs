use actix::Message;
use serde::{Deserialize, Serialize};

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct GetRestaurants {
    pub customer_location: Location,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct Restaurants {
    pub data: String,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct Order {
    pub order: OrderContent,
}

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
pub struct AuthorizePaymentRequest {
    pub customer_id: u32,
    pub price: f64,
    pub restaurant_name: String,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct ExecutePayment {
    pub customer_id: u32,
    pub price: f64,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct OrderToRestaurant {
    pub customer_id: u32,
    pub price: f64,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct OrderInProgress {
    pub customer_id: u32,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct PushNotification {
    pub notification_msg: String,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct LocationUpdate {
    pub new_location: Location,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Location {
    pub x: u16,
    pub y: u16,
}

impl Location {
    pub fn new(x: u16, y: u16) -> Location {
        Location { x, y }
    }
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct DeliveryOffer {
    pub customer_id: u32,
    pub customer_location: Location,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct DeliveryOfferAccepted {
    pub customer_id: u32,
    pub rider_id: u32,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct RiderArrivedAtCustomer {
    pub rider_id: u32,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct DeliveryDone {
    pub rider_id: u32,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct FinishDelivery {
    pub reason: String, // Reason for finishing the delivery
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum SocketMessage {
    GetRestaurants(Location),            // Location is customer_location
    Restaurants(String),                 // String is serialized json restaurants
    Order(OrderContent),                 // OrderContent is the order content
    PushNotification(String),            // String is the notification message
    LocationUpdate(Location),            // Location is the new location
    DeliveryOffer(u32, Location),        // u32 is customer_id, Location is customer location
    DeliveryOfferAccepted(u32),          // u32 is customer_id
    FinishDelivery(String),              // String is the reason for finishing the delivery
    ExecutePayment(u32, f64),            // u32 is customer_id, f64 is amount
    AuthorizePayment(u32, f64, String), // u32 is customer_id, f64 is amount, String is restaurant name
    PaymentDenied(u32, f64, String), // u32 is customer_id, f64 is amount, String is restaurant name
    PaymentAuthorized(u32, f64, String), // u32 is customer_id, f64 is amount
    PaymentExecuted(u32, f64),       // u32 is customer_id, f64 is amount
    PrepareOrder(u32, f64),          // u32 is customer_id, f64 is price
    OrderInProgress(u32),            // u32 is customer_id
    OrderCalcelled(u32),             // u32 is customer_id
    OrderReady(u32, Location),       // u32 is customer_id
    InformLocation(Location, String), // Location is the new location, String is the restaurant name
    RegisterPaymentSystem,
    RiderArrivedAtCustomer,
    DeliveryDone,
}
