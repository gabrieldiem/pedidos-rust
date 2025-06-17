use actix::{Actor, ActorContext, AsyncContext, Context, Handler, ResponseActFuture, WrapFuture};
use actix::{Addr, Message, StreamHandler};
use actix_async_handler::async_handler;
use common::configuration::Configuration;
use common::constants::{DEFAULT_PR_HOST, MAX_ORDER_PRICE, MIN_ORDER_PRICE, NO_RESTAURANTS};
use common::protocol::{
    FinishDelivery, GetRestaurants, Location, Order, OrderContent, PushNotification,
    ReconnectToNewPedidosRust, SetupReconnection, SocketMessage, Stop,
};
use common::tcp::tcp_connector::TcpConnector;
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use common::udp_gateway::{InfoForUdpGatewayData, InfoForUdpGatewayRequest};
use common::utils::logger::Logger;
use rand::seq::IndexedRandom;
use rand::{Rng, rng};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader, split};
use tokio::net::UdpSocket;
use tokio_stream::wrappers::LinesStream;

#[allow(dead_code)]
pub struct Customer {
    tcp_sender: Addr<TcpSender>,
    logger: Logger,
    location: Location,
    config: Configuration,
    my_port: u32,
    peer_port: u32,
    tcp_connector: Addr<TcpConnector>,
    udp_socket: Arc<UdpSocket>,
    is_new_customer: bool,
}

impl Actor for Customer {
    type Context = Context<Self>;
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct Start;

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct ChooseRestaurant {
    pub restaurants: String,
}

#[async_handler]
impl Handler<Start> for Customer {
    type Result = ();

    async fn handle(&mut self, _msg: Start, _ctx: &mut Self::Context) -> Self::Result {
        self.is_new_customer = true;
        _ctx.address().do_send(GetRestaurants {
            customer_location: self.location,
        });
    }
}

#[async_handler]
impl Handler<Stop> for Customer {
    type Result = ();

    async fn handle(&mut self, _msg: Stop, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Stopping Customer");
        _ctx.stop();
        std::process::exit(0);
    }
}

#[async_handler]
impl Handler<GetRestaurants> for Customer {
    type Result = ();

    async fn handle(&mut self, msg: GetRestaurants, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Getting Restaurants");

        if let Err(e) = self.send_message(&SocketMessage::GetRestaurants(
            msg.customer_location,
            self.is_new_customer,
        )) {
            self.logger.error(&e.to_string());
        }
    }
}

#[async_handler]
impl Handler<ChooseRestaurant> for Customer {
    type Result = ();

    async fn handle(&mut self, msg: ChooseRestaurant, _ctx: &mut Self::Context) -> Self::Result {
        if msg.restaurants.trim() == NO_RESTAURANTS {
            _ctx.address().do_send(FinishDelivery {
                reason: "No hay restaurantes disponibles en el momento".to_string(),
            });
            return;
        }

        let restaurants: Vec<String> = msg
            .restaurants
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if restaurants.is_empty() {
            self.logger
                .warn("No se encontraron restaurantes en el mensaje.");
            return;
        }

        let chosen_restaurant = restaurants.choose(&mut rng()).unwrap();

        let raw_price: f64 = rand::rng().random_range(MIN_ORDER_PRICE..=MAX_ORDER_PRICE);
        let order_price = (raw_price * 100.0).round() / 100.0;

        let order = OrderContent::new(chosen_restaurant.clone(), order_price);
        _ctx.address().do_send(Order { order });
    }
}

#[async_handler]
impl Handler<Order> for Customer {
    type Result = ();

    async fn handle(&mut self, msg: Order, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.info(&format!(
            "I will order {} from {}",
            msg.order.amount, msg.order.restaurant
        ));

        let msg = SocketMessage::Order(msg.order);

        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
        }
    }
}

#[async_handler]
impl Handler<PushNotification> for Customer {
    type Result = ();

    async fn handle(&mut self, msg: PushNotification, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.info(&format!(
            "Update received: {}",
            msg.notification_msg.as_str()
        ));
    }
}

#[async_handler]
impl Handler<InfoForUdpGatewayRequest> for Customer {
    type Result = InfoForUdpGatewayData;

    async fn handle(
        &mut self,
        _msg: InfoForUdpGatewayRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        return InfoForUdpGatewayData {
            port: self.my_port,
            configuration: self.config.clone(),
            udp_socket: self.udp_socket.clone(),
        };
    }
}

#[async_handler]
impl Handler<SetupReconnection> for Customer {
    type Result = ();

    async fn handle(&mut self, msg: SetupReconnection, _ctx: &mut Self::Context) -> Self::Result {
        let read_half = msg.read_half;
        let tcp_sender = msg.tcp_sender;

        Customer::add_stream(LinesStream::new(BufReader::new(read_half).lines()), _ctx);
        self.tcp_sender = tcp_sender;

        self.logger.info("Reconnection established");
        self.is_new_customer = true;
    }
}

impl Handler<ReconnectToNewPedidosRust> for Customer {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: ReconnectToNewPedidosRust, _ctx: &mut Self::Context) -> Self::Result {
        let new_id = msg.new_id;
        let new_port = msg.new_port;
        self.logger.info(&format!(
            "Reconnecting to new PedidosRust with ID {} and port {}",
            new_id, new_port
        ));

        let port = self.my_port;
        let logger = self.logger.clone();
        let my_address = _ctx.address();
        let udp_socket = self.udp_socket.clone();
        let old_tcp_sender = self.tcp_sender.clone();

        Box::pin(
            async move {
                let _ = old_tcp_sender.send(Stop {}).await;

                let tcp_connector = TcpConnector::new(port, vec![new_port]);
                let stream_res = tcp_connector.connect_with_socket(udp_socket).await;
                match stream_res {
                    Ok(stream) => {
                        let (read_half, write_half) = split(stream);

                        let tcp_sender = TcpSender {
                            write_stream: Some(write_half),
                        }
                        .start();

                        let _ = my_address
                            .send(SetupReconnection {
                                tcp_sender,
                                read_half,
                            })
                            .await;
                    }
                    Err(e) => {
                        logger.error(&format!("Could not connect to stream: {}", e));
                    }
                }
            }
            .into_actor(self),
        )
    }
}

#[async_handler]
impl Handler<FinishDelivery> for Customer {
    type Result = ();

    async fn handle(&mut self, msg: FinishDelivery, _ctx: &mut Self::Context) -> Self::Result {
        self.is_new_customer = true;
        self.logger.info(msg.reason.as_str());

        let addr = _ctx.address();
        let location = self.location;
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let stdin = tokio::io::stdin();
            let mut console_reader = BufReader::new(stdin).lines();

            let logger = Logger::new(Some("[CUSTOMER]"));
            logger.info("Presiona ENTER para realizar un nuevo pedido, o escribe 'q', 'exit' o 'quit' para salir...");

            if let Ok(Some(input)) = console_reader.next_line().await {
                let input = input.trim().to_lowercase();
                if input == "q" || input == "exit" || input == "quit" {
                    addr.do_send(Stop {});
                } else {
                    addr.do_send(GetRestaurants {
                        customer_location: location,
                    });
                }
            }
        });
    }
}

impl Customer {
    pub async fn new(
        id: u32,
        logger: Logger,
    ) -> Result<Addr<Customer>, Box<dyn std::error::Error>> {
        logger.info("Starting...");

        // Setting up ports
        let config = Configuration::new()?;
        let port_pair = config.customer.infos.iter().find(|pair| pair.id == id);

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

        let local_addr: SocketAddr = match format!("{}:{}", DEFAULT_PR_HOST, my_port).parse() {
            Ok(addr) => addr,
            Err(e) => return Err(e.into()),
        };

        let udp_socket = match UdpSocket::bind(local_addr).await {
            Ok(socket) => Arc::new(socket),
            Err(e) => {
                logger.error(&format!("Could not get UDP socket: {e}"));
                return Err(e.into());
            }
        };

        // Creating actor
        let customer = Customer::create(|ctx| {
            let (read_half, write_half) = split(stream);

            Customer::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);

            let tcp_sender = TcpSender {
                write_stream: Some(write_half),
            }
            .start();

            logger.debug("Created Customer");

            let tcp_connector_actor = TcpConnector::create(|_tcp_ctx| TcpConnector {
                logger: Logger::new(Some("[TCP-CONNECTOR]")),
                source_port: my_port,
                dest_ports: dest_ports.clone(),
            });

            let customer_info = match config.customer.infos.iter().find(|c| c.id == id) {
                Some(info) => info,
                None => {
                    panic!("No se encontró el cliente con id: {}", id);
                }
            };

            let customer_location = Location::new(customer_info.x, customer_info.y);
            logger.info(&format!(
                "Customer {} started at port {} with location ({}, {})",
                id, my_port, customer_location.x, customer_location.y
            ));
            Customer {
                tcp_sender,
                logger: Logger::new(Some("[CUSTOMER]")),
                location: customer_location,
                config,
                my_port,
                peer_port: peer_port as u32,
                tcp_connector: tcp_connector_actor,
                udp_socket,
                is_new_customer: true,
            }
        });
        Ok(customer)
    }

    #[allow(unreachable_patterns)]
    fn dispatch_message(&mut self, line_read: String, ctx: &mut <Customer as Actor>::Context) {
        let parsed_line = serde_json::from_str(&line_read);
        match parsed_line {
            Ok(message) => match message {
                SocketMessage::Restaurants(restaurant) => {
                    ctx.address().do_send(ChooseRestaurant {
                        restaurants: restaurant,
                    });
                }
                SocketMessage::PushNotification(notification_msg) => {
                    ctx.address().do_send(PushNotification { notification_msg });
                }
                SocketMessage::FinishDelivery(reason) => {
                    ctx.address().do_send(FinishDelivery { reason });
                }
                _ => {
                    self.logger
                        .warn(&format!("Unrecognized message: {:?}", message));
                }
            },
            Err(e) => {
                self.logger.error(&format!(
                    "Failed to deserialize message: {}. Message received: {}",
                    e, line_read
                ));
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

impl StreamHandler<Result<String, io::Error>> for Customer {
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

    fn finished(&mut self, _ctx: &mut Self::Context) {
        self.logger.warn("Detected PedidosRust connection down");
    }
}
