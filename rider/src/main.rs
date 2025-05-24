use actix::{Actor, ActorContext, Addr, AsyncContext, Context, Handler, Message, StreamHandler};
use actix_async_handler::async_handler;

use common::constants::{DEFAULT_PR_HOST, DEFAULT_PR_PORT};
use common::protocol::{DeliveryOffer, Location, LocationUpdate, SocketMessage};
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use std::io;
use tokio::io::{AsyncBufReadExt, BufReader, split};
use tokio::net::TcpStream;
use tokio::time::{Duration, sleep};
use tokio_stream::wrappers::LinesStream;

struct Rider {
    tcp_sender: Addr<TcpSender>,
    logger: Logger,
    location: Location,
    customer_location: Option<Location>,
    busy: bool,
}

impl Actor for Rider {
    type Context = Context<Self>;
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct Start;

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct Stop;

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct GoToCustomerLocation;

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct DeliverOrderToCustomerHands;

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

            if let Err(e) = self.send_message(&SocketMessage::RiderArrivedAtCustomer) {
                self.logger.error(&e.to_string());
                return;
            }

            _ctx.address().do_send(DeliverOrderToCustomerHands);
        }
    }
}

#[async_handler]
impl Handler<DeliverOrderToCustomerHands> for Rider {
    type Result = ();

    async fn handle(
        &mut self,
        _msg: DeliverOrderToCustomerHands,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.logger.info("Delivering order to customer hands...");
        sleep(Duration::from_millis(4000)).await;

        self.logger.info("Delivery done!");

        if let Err(e) = self.send_message(&SocketMessage::DeliveryDone) {
            self.logger.error(&e.to_string());
            return;
        }

        self.busy = false;
        _ctx.address().do_send(Stop);
    }
}

#[async_handler]
impl Handler<DeliveryOffer> for Rider {
    type Result = ();

    async fn handle(&mut self, msg: DeliveryOffer, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Got DeliveryOffer");
        let will_accept = true; // can be changed to random afterward

        if will_accept && !self.busy {
            self.logger.debug(&format!(
                "Delivery accepted for customer {}",
                msg.customer_id
            ));

            self.customer_location = Some(msg.customer_location);
            self.busy = true;

            let msg = SocketMessage::DeliveryOfferAccepted(msg.customer_id);
            if let Err(e) = self.send_message(&msg) {
                self.logger.error(&e.to_string());
                return;
            }
            _ctx.address().do_send(GoToCustomerLocation);
        }
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

impl Rider {
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

#[actix_rt::main]
async fn main() -> io::Result<()> {
    let logger = Logger::new(Some("[RIDER]"));
    logger.info("Starting...");

    let server_sockeaddr_str = format!("{}:{}", DEFAULT_PR_HOST, DEFAULT_PR_PORT);
    let stream = TcpStream::connect(server_sockeaddr_str.clone()).await?;

    logger.info(&format!("Using address {}", stream.local_addr()?));
    logger.info(&format!("Connected to server {}", server_sockeaddr_str));

    let rider = Rider::create(|ctx| {
        let (read_half, write_half) = split(stream);

        Rider::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);
        let tcp_sender = TcpSender {
            write_stream: Some(write_half),
        }
        .start();

        logger.debug("Created Rider");
        let starting_location = Location::new(5, 6);
        Rider {
            tcp_sender,
            logger: Logger::new(Some("[RIDER]")),
            location: starting_location,
            customer_location: None,
            busy: false,
        }
    });

    match rider.send(Start).await {
        Ok(_) => {}
        Err(e) => logger.error(&format!("Could not start actor: {e}")),
    }

    actix_rt::signal::ctrl_c().await?;

    Ok(())
}
