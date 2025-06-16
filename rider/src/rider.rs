use actix::{Actor, ActorContext, AsyncContext, Context, Handler};
use actix::{Addr, Message, StreamHandler};
use actix_async_handler::async_handler;
use common::configuration::Configuration;
use common::constants::{DELIVERY_ACCEPT_PROBABILITY, MAX_ORDER_PRICE, MIN_ORDER_PRICE, NO_RESTAURANTS};
use common::protocol::{DeliveryOffer, DeliveryOfferConfirmed, FinishDelivery, GetRestaurants, Location, LocationUpdate, Order, OrderContent, PushNotification, SocketMessage, Stop};
use common::tcp::tcp_connector::TcpConnector;
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use rand::seq::IndexedRandom;
use rand::{Rng, rng, thread_rng};
use std::io;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader, split};
use tokio::time::sleep;
use tokio_stream::wrappers::LinesStream;


#[allow(dead_code)]
pub struct Rider {
    tcp_sender: Addr<TcpSender>,
    logger: Logger,
    location: Location,
    config: Configuration,
    my_port: u32,
    peer_port: u32,
    tcp_connector: Addr<TcpConnector>,
    customer_location: Option<Location>,
    busy: bool,
    customer_id: Option<u32>
}

impl Actor for Rider {
    type Context = Context<Self>;
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct Start;

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct GoToCustomerLocation;

#[async_handler]
impl Handler<Start> for Rider {
    type Result = ();

    async fn handle(&mut self, _msg: Start, _ctx: &mut Self::Context) -> Self::Result {
        _ctx.address().do_send(LocationUpdate {
            new_location: self.location,
        });
    }
}

#[async_handler]
impl Handler<Stop> for Rider {
    type Result = ();

    async fn handle(&mut self, _msg: Stop, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Stopping Rider");
        _ctx.stop();
        std::process::exit(0);
    }
}

#[async_handler]
impl Handler<LocationUpdate> for Rider {
    type Result = ();

    async fn handle(&mut self, msg: LocationUpdate, _ctx: &mut Self::Context) -> Self::Result {
        let new_location = msg.new_location;

        self.logger.debug(&format!(
            "My updated location is: ({}, {})",
            msg.new_location.x, msg.new_location.y
        ));

        let msg = SocketMessage::LocationUpdate(new_location);
        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<GoToCustomerLocation> for Rider {
    type Result = ();

    async fn handle(
        &mut self,
        _msg: GoToCustomerLocation,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.logger.info("Going to customer house...");
        sleep(Duration::from_millis(4000)).await;

        if let Some(customer_location) = self.customer_location {
            self.location = customer_location;

            _ctx.address().do_send(LocationUpdate {
                new_location: self.location,
            });

            self.logger.info(&format!(
                "Arrived at customer house ({}, {})",
                customer_location.x, customer_location.y
            ));

            self.logger.info("Delivery done!");
            self.busy = false;
            self.customer_location = None;

            if let Err(e) =
                self.send_message(&SocketMessage::DeliveryDone(self.customer_id.unwrap()))
            {
                self.logger.error(&e.to_string());
                return;
            }
        }
    }
}

#[async_handler]
impl Handler<DeliveryOffer> for Rider {
    type Result = ();

    async fn handle(&mut self, msg: DeliveryOffer, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Got DeliveryOffer");
        let will_accept = rand::random::<f32>() < DELIVERY_ACCEPT_PROBABILITY;

        let sleep_millis = rand::random::<u64>() % 2000 + 1000;
        sleep(Duration::from_millis(sleep_millis)).await;

        if will_accept && !self.busy {
            self.logger.debug(&format!(
                "Delivery accepted for customer {}",
                msg.customer_id
            ));

            let msg = SocketMessage::DeliveryOfferAccepted(msg.customer_id);
            if let Err(e) = self.send_message(&msg) {
                self.logger.error(&e.to_string());
                return;
            }
        } else {
            self.logger.debug(&format!(
                "Delivery offer for customer {} denied",
                msg.customer_id
            ));
            // Hace falta hacer algo si se niega la oferta?
            //let msg = SocketMessage::DeliveryOfferDenied(self.customer_id.unwrap());
            //if let Err(e) = self.send_message(&msg) {
            //    self.logger.error(&e.to_string());
            //    return;
            //}
        }
    }
}

#[async_handler]
impl Handler<DeliveryOfferConfirmed> for Rider {
    type Result = ();

    async fn handle(
        &mut self,
        msg: DeliveryOfferConfirmed,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.logger.debug(&format!(
            "Delivery offer confirmed for customer {}",
            msg.customer_id
        ));

        self.customer_location = Some(msg.customer_location);
        self.busy = true;
        self.customer_id = Some(msg.customer_id);

        _ctx.address().do_send(GoToCustomerLocation);
    }
}

impl Rider {
    pub async fn new(
        id: u32,
        logger: Logger,
    ) -> Result<Addr<Rider>, Box<dyn std::error::Error>> {
        logger.info("Starting...");

        // Setting up ports
        let config = Configuration::new()?;
        let port_pair = config.rider.infos.iter().find(|pair| pair.id == id);

        let port_pair = match port_pair {
            Some(pair) => pair,
            None => {
                return Err(format!("Could not find port in configuration for id: {}", id).into());
            }
        };

        let my_port = port_pair.port;
        let dest_ports: Vec<u32> = config
            .pedidos_rust
            .infos
            .iter()
            .map(|pair| pair.port)
            .collect();

        // Setting up connection
        let tcp_connector = TcpConnector::new(my_port, dest_ports.clone());
        let stream = tcp_connector.connect().await?;
        let peer_address = stream.peer_addr()?;
        let peer_port = peer_address.port();

        // Creating actor
        let rider = Rider::create(|ctx| {
            let (read_half, write_half) = split(stream);

            Rider::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);

            let tcp_sender = TcpSender {
                write_stream: Some(write_half),
            }
                .start();

            logger.debug("Created Rider");

            let tcp_connector_actor = TcpConnector::create(|_tcp_ctx| TcpConnector {
                logger: Logger::new(Some("[TCP-CONNECTOR]")),
                source_port: my_port,
                dest_ports: dest_ports.clone(),
            });

            let rider_info = match config.rider.infos.iter().find(|c| c.id == id) {
                Some(info) => info,
                None => {
                    panic!("No se encontró el cliente con id: {}", id);
                }
            };

            let rider_location = Location::new(rider_info.x, rider_info.y);
            logger.info(&format!(
                "Rider {} started at port {} with location ({}, {})",
                id, my_port, rider_location.x, rider_location.y
            ));
            Rider {
                tcp_sender,
                logger: Logger::new(Some("[CUSTOMER]")),
                location: rider_location,
                config,
                my_port,
                peer_port: peer_port as u32,
                tcp_connector: tcp_connector_actor,
                customer_location: None,
                busy: false,
                customer_id: None,
            }
        });
        Ok(rider)
    }

    #[allow(unreachable_patterns)]
    fn dispatch_message(&mut self, line_read: String, ctx: &mut <Rider as Actor>::Context) {
        let parsed_line = serde_json::from_str(&line_read);
        match parsed_line {
            Ok(message) => match message {
                SocketMessage::DeliveryOffer(customer_id, customer_location) => {
                    ctx.address().do_send(DeliveryOffer {
                        customer_id,
                        customer_location,
                    });
                }
                SocketMessage::DeliveryOfferConfirmed(customer_id, customer_location) => {
                    ctx.address().do_send(DeliveryOfferConfirmed {
                        customer_id,
                        customer_location,
                    });
                }
                _ => {
                    self.logger
                        .warn(&format!("Unrecognized message: {:?}", message));
                }
            },
            Err(e) => {
                self.logger
                    .error(&format!("Failed to deserialize message: {}", e));
            }
        }
    }

    fn send_message(&self, socket_message: &SocketMessage) -> Result<(), String> {
        // message serialization
        let msg_to_send = match serde_json::to_string(socket_message) {
            Ok(ok_result) => ok_result,
            Err(e) => {
                return Err(format!("Failed to serialize message: {}", e));
            }
        };

        // sending message
        if let Err(e) = self.tcp_sender.try_send(TcpMessage {
            data: msg_to_send + "\n",
        }) {
            return Err(format!("Failed to write to stream: {}", e));
        }

        Ok(())
    }
}

impl StreamHandler<Result<String, io::Error>> for Rider {
    fn handle(&mut self, msg_read: Result<String, io::Error>, ctx: &mut Self::Context) {
        match msg_read {
            Ok(line_read) => match line_read.strip_suffix("\n") {
                Some(line_stripped) => {
                    self.dispatch_message(line_stripped.to_string(), ctx);
                }
                None => {
                    if line_read.is_empty() {
                        self.logger.warn("Empty line received");
                    } else {
                        self.dispatch_message(line_read, ctx);
                    }
                }
            },
            Err(e) => {
                self.logger
                    .error(&format!("Failed to read from stream: {}", e));
            }
        }
    }
}
