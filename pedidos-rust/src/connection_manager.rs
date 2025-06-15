use crate::client_connection::ClientConnection;
use crate::messages::{
    AuthorizePayment, FindRider, OrderCancelled, OrderReady, OrderRequest, PaymentAuthorized,
    PaymentDenied, PaymentExecuted, RegisterCustomer, RegisterPaymentSystem, RegisterRestaurant,
    RegisterRider, SendNotification, SendRestaurantList,
};
use crate::nearby_entitys::{closest_riders, nearby_restaurants};
use actix::{Actor, Addr, AsyncContext, Context, Handler, ResponseActFuture, WrapFuture};
use actix_async_handler::async_handler;
use common::constants::{N_RIDERS_TO_NOTIFY, NO_RESTAURANTS};
use common::protocol::{
    AuthorizePaymentRequest, DeliveryDone, DeliveryOffer, DeliveryOfferAccepted, ExecutePayment,
    FinishDelivery, Location, OrderToRestaurant, PushNotification, Restaurants,
    RiderArrivedAtCustomer,
};
use common::utils::logger::Logger;
use std::collections::{HashMap, VecDeque};

type CustomerId = u32;
type RiderId = u32;
type RestaurantId = u32;
type RestaurantName = String;

pub struct RiderData {
    pub address: Addr<ClientConnection>,
    pub location: Option<Location>,
}

pub struct CustomerData {
    pub address: Addr<ClientConnection>,
    pub location: Location,
    pub order_price: Option<f64>,
}

impl CustomerData {
    pub fn new(
        address: Addr<ClientConnection>,
        location: Location,
        order_price: Option<f64>,
    ) -> CustomerData {
        CustomerData {
            address,
            location,
            order_price,
        }
    }
}

#[allow(dead_code)]
pub struct RestaurantData {
    pub address: Addr<ClientConnection>,
    pub id: RestaurantId,
    pub location: Location,
}

impl RestaurantData {
    pub fn new(
        address: Addr<ClientConnection>,
        id: RestaurantId,
        location: Location,
    ) -> RestaurantData {
        RestaurantData {
            address,
            id,
            location,
        }
    }
}

pub struct ConnectionManager {
    pub logger: Logger,
    pub riders: HashMap<RiderId, RiderData>,
    pub customers: HashMap<CustomerId, CustomerData>,
    pub orders_in_process: HashMap<RiderId, CustomerId>,
    pub pending_delivery_requests: VecDeque<FindRider>,
    pub restaurants: HashMap<RestaurantName, RestaurantData>,
    pub payment_system: Option<Addr<ClientConnection>>,
}

impl ConnectionManager {
    pub fn new() -> ConnectionManager {
        ConnectionManager {
            logger: Logger::new(Some("[CONNECTION-MANAGER]")),
            riders: HashMap::new(),
            customers: HashMap::new(),
            orders_in_process: HashMap::new(),
            pending_delivery_requests: VecDeque::new(),
            restaurants: HashMap::new(),
            payment_system: None,
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
            .or_insert(CustomerData::new(msg.address, msg.location, None));
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
            .or_insert(RestaurantData::new(msg.address, msg.id, msg.location));
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
                let nearby = nearby_restaurants(&customer.location, &self.restaurants);

                let restaurant_list = if nearby.is_empty() {
                    NO_RESTAURANTS.to_string()
                } else {
                    nearby.into_iter().cloned().collect::<Vec<_>>().join(", ")
                };

                self.logger
                    .debug(&format!("Restaurant list to send: {}", restaurant_list));

                customer.address.do_send(Restaurants {
                    data: restaurant_list,
                });
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
            // TODO: FinishDelivery al customer con el mensaje de que no anda el sistema de pago y lo vuelva a intentar mas tarde
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

        if let Some(customer) = self.customers.get(&msg.customer_id) {
            customer.address.do_send(PushNotification {
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

        if let Some(customer) = self.customers.get(&msg.customer_id) {
            let reason_msg = format!(
                "Payment of {} denied for your order at {}.",
                msg.amount, msg.restaurant_name
            );
            customer.address.do_send(FinishDelivery {
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
        } else {
            self.logger
                .warn("Failed to find customer data when preparing order");
        }
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
        match self.customers.get(&msg.recipient_id) {
            Some(customer) => {
                customer.address.do_send(PushNotification {
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
        if let Some(customer) = self.customers.get(&msg.customer_id) {
            let notification_msg =
                "Your order is ready! Finding a rider for you order...".to_string();
            customer.address.do_send(PushNotification {
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
        if let Some(customer) = self.customers.get(&msg.customer_id) {
            customer.address.do_send(FinishDelivery {
                reason: "Delivery cancelled due to lack of stock".to_string(),
            });
        } else {
            self.logger
                .warn("Failed to find customer data when cancelling order");
        }
    }
}

// any rider will do
// TODO: bug when connecting multiple riders, given that only one is gotten
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

        let closest = closest_riders(&msg.restaurant_location, &self.riders, N_RIDERS_TO_NOTIFY);

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
        match self.customers.get(&msg.customer_id) {
            Some(customer) => {
                let notification_msg =
                    format!("The rider {} will deliver your order", msg.rider_id);
                customer
                    .address
                    .do_send(PushNotification { notification_msg });
                self.orders_in_process.insert(msg.rider_id, msg.customer_id);
            }
            None => {
                self.logger.warn("Failed finding customer");
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
            Some(customer_id) => {
                match self.customers.get(customer_id) {
                    Some(customer) => {
                        let notification_msg = "The rider is outside! Pick up the order".to_owned();
                        customer
                            .address
                            .do_send(PushNotification { notification_msg });
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
    }
}

#[async_handler]
impl Handler<DeliveryDone> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: DeliveryDone, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Entering DeliveryDone handler");
        let Some(&customer_id) = self.orders_in_process.get(&msg.rider_id) else {
            self.logger.warn("Failed finding order in progress");
            return;
        };

        let Some(customer) = self.customers.get(&customer_id) else {
            self.logger.warn("Failed finding customer");
            return;
        };

        if self.orders_in_process.remove(&msg.rider_id).is_none() {
            self.logger
                .warn("Expected order to exist for removal but it was missing");
            return;
        }

        self.logger.debug("Removed finished order");

        let execute_payment_msg = ExecutePayment {
            customer_id,
            price: customer.order_price.unwrap_or(0.0),
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

        if let Some(customer) = self.customers.get(&msg.customer_id) {
            let reason_msg = format!("Payment successfully executed for order of {}.", msg.amount,);
            customer.address.do_send(FinishDelivery {
                reason: reason_msg.clone(),
            });
        } else {
            self.logger
                .warn("Failed to find customer data when denying payment");
        }
        self.process_pending_requests();
    }
}
