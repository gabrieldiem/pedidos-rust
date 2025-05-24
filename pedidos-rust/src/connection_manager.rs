use crate::client_connection::ClientConnection;
use crate::messages::{FindRider, RegisterCustomer, RegisterRider};
use actix::{Actor, Addr, Context, Handler};
use actix_async_handler::async_handler;
use common::protocol::{
    DeliveryDone, DeliveryOffer, DeliveryOfferAccepted, FinishDelivery, Location, PushNotification,
    RiderArrivedAtCustomer,
};
use common::utils::logger::Logger;
use std::collections::{HashMap, VecDeque};

type CustomerId = u32;
type RiderId = u32;

pub struct CustomerData {
    pub address: Addr<ClientConnection>,
    pub location: Location,
}

impl CustomerData {
    pub fn new(address: Addr<ClientConnection>, location: Location) -> CustomerData {
        CustomerData { address, location }
    }
}

pub struct ConnectionManager {
    pub logger: Logger,
    pub riders: HashMap<RiderId, Addr<ClientConnection>>,
    pub customers: HashMap<CustomerId, CustomerData>,
    pub orders_in_process: HashMap<RiderId, CustomerId>,
    pub pending_delivery_requests: VecDeque<FindRider>,
}

impl ConnectionManager {
    pub fn new() -> ConnectionManager {
        ConnectionManager {
            logger: Logger::new(Some("[CONNECTION-MANAGER]")),
            riders: HashMap::new(),
            customers: HashMap::new(),
            orders_in_process: HashMap::new(),
            pending_delivery_requests: VecDeque::new(),
        }
    }

    fn process_pending_requests(&mut self) {
        while let Some(pending_request) = self.pending_delivery_requests.pop_front() {
            if let Some(rider) = self.riders.values().find(|_rider| true) {
                self.logger
                    .debug("Assigned pending delivery request to newly registered rider");
                match self.customers.get(&pending_request.customer_id) {
                    Some(customer) => {
                        rider.do_send(DeliveryOffer {
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
        self.riders.entry(msg.id).or_insert(msg.address);
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
            .or_insert(CustomerData::new(msg.address, msg.location));
    }
}

#[async_handler]
impl Handler<FindRider> for ConnectionManager {
    type Result = ();

    async fn handle(&mut self, msg: FindRider, _ctx: &mut Self::Context) -> Self::Result {
        // any rider will do
        match self.riders.values().find(|_rider| true) {
            Some(found_rider) => match self.customers.get(&msg.customer_id) {
                Some(customer) => {
                    found_rider.do_send(DeliveryOffer {
                        customer_id: msg.customer_id,
                        customer_location: customer.location,
                    });
                }
                None => {
                    self.logger
                        .warn("Failed to find customer data when finding rider");
                }
            },
            None => {
                self.logger
                    .warn("No rider found, adding to pending requests");
                self.pending_delivery_requests.push_back(msg);
            }
        };
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
        let possible_customer_id = self.orders_in_process.get(&msg.rider_id).copied();

        match possible_customer_id {
            Some(customer_id) => {
                match self.customers.get(&customer_id) {
                    Some(customer) => match self.orders_in_process.remove(&msg.rider_id) {
                        Some(_) => {
                            self.logger.debug("Removed finished order");
                            customer.address.do_send(FinishDelivery);
                        }
                        None => {
                            self.logger.debug("No order found to be removed");
                        }
                    },

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
