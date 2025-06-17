use actix::{Actor, ActorContext, AsyncContext, Context, Handler};
use actix::{Addr, Message, StreamHandler};
use actix_async_handler::async_handler;
use common::configuration::Configuration;
use common::constants::{MAX_ORDER_DURATION, MIN_ORDER_DURATION, ORDER_REJECTED_PROBABILITY};
use common::protocol::{Location, SocketMessage, Stop};
use common::tcp::tcp_connector::TcpConnector;
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use rand::Rng;
use std::io;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader, split};
use tokio_stream::wrappers::LinesStream;

#[allow(dead_code)]
pub struct Restaurant {
    tcp_sender: Addr<TcpSender>,
    logger: Logger,
    location: Location,
    config: Configuration,
    my_port: u32,
    peer_port: u32,
    tcp_connector: Addr<TcpConnector>,
    name: String,
}

impl Actor for Restaurant {
    type Context = Context<Self>;
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct Start;

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct InformLocation;

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct PrepareOrder {
    pub customer_id: u32,
    pub price: f64,
}

#[async_handler]
impl Handler<Start> for Restaurant {
    type Result = ();

    async fn handle(&mut self, _msg: Start, _ctx: &mut Self::Context) -> Self::Result {
        _ctx.address().do_send(InformLocation {});
    }
}

#[async_handler]
impl Handler<Stop> for Restaurant {
    type Result = ();

    async fn handle(&mut self, _msg: Stop, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Stopping Restaurant");
        _ctx.stop();
        std::process::exit(0);
    }
}

#[async_handler]
impl Handler<InformLocation> for Restaurant {
    type Result = ();

    async fn handle(&mut self, _msg: InformLocation, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug(&format!(
            "Informing my location to PedidosRust: ({}, {})",
            self.location.x, self.location.y
        ));

        let msg = SocketMessage::InformLocation(self.location, self.name.clone());
        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<PrepareOrder> for Restaurant {
    type Result = ();

    async fn handle(&mut self, msg: PrepareOrder, _ctx: &mut Self::Context) {
        self.logger.info(&format!(
            "Order received from customer {} with price {}",
            msg.customer_id, msg.price
        ));

        let response = SocketMessage::OrderInProgress(msg.customer_id);
        if let Err(e) = self.send_message(&response) {
            self.logger.error(&e.to_string());
            return;
        }

        let secs = rand::rng().random_range(MIN_ORDER_DURATION..=MAX_ORDER_DURATION);
        _ctx.notify_later(
            ContinueOrder {
                customer_id: msg.customer_id,
                price: msg.price,
            },
            Duration::from_secs(secs),
        );
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ContinueOrder {
    pub customer_id: u32,
    pub price: f64,
}

#[async_handler]
impl Handler<ContinueOrder> for Restaurant {
    type Result = ();

    async fn handle(&mut self, msg: ContinueOrder, _ctx: &mut Self::Context) {
        let accepted = rand::random::<f32>() > ORDER_REJECTED_PROBABILITY;

        if !accepted {
            self.logger.info(&format!(
                "Order from client {} rejected due to lack of stock",
                msg.customer_id
            ));
            let response = SocketMessage::OrderCalcelled(msg.customer_id);
            if let Err(e) = self.send_message(&response) {
                self.logger.error(&e.to_string());
            }
            return;
        }

        self.logger.info(&format!(
            "Order from client {} with price {} is ready",
            msg.customer_id, msg.price
        ));
        let response = SocketMessage::OrderReady(msg.customer_id, self.location);
        if let Err(e) = self.send_message(&response) {
            self.logger.error(&e.to_string());
        }
    }
}

impl Restaurant {
    pub async fn new(
        id: u32,
        logger: Logger,
    ) -> Result<Addr<Restaurant>, Box<dyn std::error::Error>> {
        logger.info("Starting...");

        // Setting up ports
        let config = Configuration::new()?;
        let port_pair = config.restaurant.infos.iter().find(|pair| pair.id == id);

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
        let restaurant = Restaurant::create(|ctx| {
            let (read_half, write_half) = split(stream);

            Restaurant::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);

            let tcp_sender = TcpSender {
                write_stream: Some(write_half),
            }
            .start();

            logger.debug("Created Restaurant");

            let tcp_connector_actor = TcpConnector::create(|_tcp_ctx| TcpConnector {
                logger: Logger::new(Some("[TCP-CONNECTOR]")),
                source_port: my_port,
                dest_ports: dest_ports.clone(),
            });

            let restaurant_info = match config.restaurant.infos.iter().find(|c| c.id == id) {
                Some(info) => info,
                None => {
                    panic!("No se encontró el cliente con id: {}", id);
                }
            };

            let restaurant_location = Location::new(restaurant_info.x, restaurant_info.y);
            logger.info(&format!(
                "Restaurant {} started at port {} with location ({}, {})",
                id, my_port, restaurant_location.x, restaurant_location.y
            ));
            Restaurant {
                tcp_sender,
                logger: Logger::new(Some("[CUSTOMER]")),
                location: restaurant_location,
                config: config.clone(),
                my_port,
                peer_port: peer_port as u32,
                tcp_connector: tcp_connector_actor,
                name: restaurant_info.name.clone(),
            }
        });
        Ok(restaurant)
    }

    #[allow(unreachable_patterns)]
    fn dispatch_message(&mut self, line_read: String, ctx: &mut <Restaurant as Actor>::Context) {
        let parsed_line = serde_json::from_str(&line_read);
        match parsed_line {
            Ok(message) => match message {
                SocketMessage::PrepareOrder(customer_id, price) => {
                    ctx.address().do_send(PrepareOrder { customer_id, price });
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

impl StreamHandler<Result<String, io::Error>> for Restaurant {
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
