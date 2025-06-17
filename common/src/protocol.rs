use crate::tcp::tcp_sender::TcpSender;
use actix::{Addr, Message};
use serde::{Deserialize, Serialize};
use tokio::io::ReadHalf;
use tokio::net::TcpStream;

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct Stop {}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct Reconnect {
    pub current_connected_port: u32,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct ConnectTo {
    pub port: u32,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct GetRestaurants {
    pub customer_location: Location,
    pub new_customer: bool,
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

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct LocationUpdateForRider {
    pub new_location: Location,
    pub rider_id: u32,
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
pub struct DeliveryOfferDenied {
    pub rider_id: u32,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct DeliveryOfferConfirmed {
    pub customer_id: u32,
    pub customer_location: Location,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct DeliveryOfferAlreadyAssigned {
    pub customer_id: u32,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct RiderArrivedAtCustomer {
    pub rider_id: u32,
    pub customer_id: u32,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct DeliveryDone {
    pub rider_id: u32,
    pub customer_id: u32,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct FinishDelivery {
    pub reason: String, // Reason for finishing the delivery
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct IsConnectionReady {}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct ConnectionAvailable {}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct ConnectionNotAvailable {
    pub port_to_connect: u32,
}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct ElectionCall {}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct ElectionOk {}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct ElectionCoordinator {}

#[derive(Message, Serialize, Deserialize, Debug, Clone)]
#[rtype(result = "()")]
pub struct SendUpdateCustomerData {
    pub customer_id: u32,
    pub location: Location,
    pub order_price: Option<f64>,
}

#[derive(Message, Serialize, Deserialize, Debug, Clone)]
#[rtype(result = "()")]
pub struct SendUpdateRestaurantData {
    pub restaurant_name: String,
    pub location: Location,
}

#[derive(Message, Serialize, Deserialize, Debug, Clone)]
#[rtype(result = "()")]
pub struct SendUpdateRiderData {
    pub rider_id: u32,
    pub location: Option<Location>,
}

#[derive(Message, Serialize, Deserialize, Debug, Clone)]
#[rtype(result = "()")]
pub struct SendUpdatePaymentSystemData {
    pub port: u32,
}

#[derive(Message, Serialize, Deserialize, Debug, Clone)]
#[rtype(result = "()")]
pub struct SendUpdateOrderInProgressData {
    pub customer_id: u32,
    pub customer_location: Location,
    pub order_price: Option<f64>,
    pub rider_id: Option<u32>,
}

#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct SendRemoveOrderInProgressData {
    pub customer_id: u32,
}

#[derive(Message, Serialize, Deserialize, Debug, Clone)]
#[rtype(result = "()")]
pub struct SendPushPendingDeliveryRequest {
    pub customer_id: u32,
    pub restaurant_location: Location,
    pub to_front: bool,
}

#[derive(Message, Serialize, Deserialize, Debug, Clone)]
#[rtype(result = "()")]
pub struct SendPopPendingDeliveryRequest {}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct LeaderQuery {}

#[derive(Message, Serialize, Deserialize, Debug)]
#[rtype(result = "()")]
pub struct ReconnectToNewPedidosRust {
    pub new_id: u32,
    pub new_port: u32,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct SetupReconnection {
    pub tcp_sender: Addr<TcpSender>,
    pub read_half: ReadHalf<TcpStream>,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct UpdatePaymentSystemData {
    pub port: u32,
}

pub const UNKNOWN_LEADER: u32 = 0;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum SocketMessage {
    GetRestaurants(Location, bool), // Location is customer_location, bool is new_customer
    Restaurants(String),            // String is serialized json restaurants
    Order(OrderContent),            // OrderContent is the order content
    PushNotification(String),       // String is the notification message
    LocationUpdate(Location),       // Location is the new location
    DeliveryOffer(u32, Location),   // u32 is customer_id, Location is customer location
    DeliveryOfferAccepted(u32),     // u32 is customer_id
    DeliveryOfferConfirmed(u32, Location), // u32 is customer_id, Location is customer location
    FinishDelivery(String),         // String is the reason for finishing the delivery
    ExecutePayment(u32, f64),       // u32 is customer_id, f64 is amount
    AuthorizePayment(u32, f64, String), // u32 is customer_id, f64 is amount, String is restaurant name
    PaymentDenied(u32, f64, String), // u32 is customer_id, f64 is amount, String is restaurant name
    PaymentAuthorized(u32, f64, String), // u32 is customer_id, f64 is amount
    PaymentExecuted(u32, f64),       // u32 is customer_id, f64 is amount
    PrepareOrder(u32, f64),          // u32 is customer_id, f64 is price
    OrderInProgress(u32),            // u32 is customer_id
    OrderCalcelled(u32),             // u32 is customer_id
    OrderReady(u32, Location),       // u32 is customer_id
    InformLocation(Location, String), // Location is the new location, String is the restaurant name
    RegisterPaymentSystem(u32),      // u32 is payment system port
    RiderArrivedAtCustomer,
    DeliveryDone(u32), // u32 is customer_id
    IsConnectionReady,
    ConnectionAvailable,
    ConnectionNotAvailable(u32), // u32 is the port of the available connection, can be UNKNOWN_LEADER
    ConnectionAvailableForPeer,
    ElectionCall,
    ElectionOk,
    ElectionCoordinator,
    UpdateCustomerData(u32, Location, Option<f64>),
    UpdateRestaurantData(String, Location),
    UpdateRiderData(u32, Option<Location>),
    UpdateOrderInProgressData(u32, Location, Option<f64>, Option<u32>),
    RemoveOrderInProgressData(u32),
    PushPendingDeliveryRequest(u32, Location, bool),
    PopPendingDeliveryRequest,
    UpdatePaymentSystemData(u32), // u32 is port of the payment system data
    LeaderQuery,
    LeaderData(u32),               // u32 is leader canonical port
    ReconnectionMandate(u32, u32), // (u32, u32) = (new_leader_id, new_leader_port)
}
