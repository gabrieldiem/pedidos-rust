use actix::{Actor, ActorContext, AsyncContext, Context, Handler, ResponseActFuture, WrapFuture};
use actix::{Addr, Message, StreamHandler};
use actix_async_handler::async_handler;
use common::configuration::Configuration;
use common::constants::{
    DEFAULT_PAYMENT_PORT, DEFAULT_PR_HOST, PAYMENT_DURATION, PAYMENT_REJECTED_PROBABILITY,
};
use common::protocol::{ReconnectToNewPedidosRust, SetupReconnection, SocketMessage, Stop};
use common::tcp::tcp_connector::TcpConnector;
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use common::udp_gateway::{InfoForUdpGatewayData, InfoForUdpGatewayRequest};
use common::utils::logger::Logger;
use rand::random;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader, split};
use tokio::net::UdpSocket;
use tokio::time::sleep;
use tokio_stream::wrappers::LinesStream;

#[allow(dead_code)]
pub struct PaymentSystem {
    tcp_sender: Addr<TcpSender>,
    logger: Logger,
    config: Configuration,
    my_port: u32,
    peer_port: u32,
    tcp_connector: Addr<TcpConnector>,
    udp_socket: Arc<UdpSocket>,
}

impl Actor for PaymentSystem {
    type Context = Context<Self>;
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct Start;

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct RegisterPaymentSystem;

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct ExecuteAutorization {
    pub customer_id: u32,
    pub amount: f64,
    pub restaurant_name: String,
}

#[async_handler]
impl Handler<Start> for PaymentSystem {
    type Result = ();

    async fn handle(&mut self, _msg: Start, _ctx: &mut Self::Context) -> Self::Result {
        _ctx.address().do_send(RegisterPaymentSystem {});
    }
}

#[async_handler]
impl Handler<Stop> for PaymentSystem {
    type Result = ();

    async fn handle(&mut self, _msg: Stop, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Stopping PaymentSystem");
        _ctx.stop();
        std::process::exit(0);
    }
}

#[async_handler]
impl Handler<RegisterPaymentSystem> for PaymentSystem {
    type Result = ();

    async fn handle(
        &mut self,
        _msg: RegisterPaymentSystem,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.logger
            .debug("Registering Payment System to PedidosRust...");

        let msg = SocketMessage::RegisterPaymentSystem(self.my_port);
        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

#[async_handler]
impl Handler<ExecuteAutorization> for PaymentSystem {
    type Result = ();

    async fn handle(&mut self, msg: ExecuteAutorization, _ctx: &mut Self::Context) {
        self.logger.info(&format!(
            "Executing authorization for client {} with price {}...",
            msg.customer_id, msg.amount
        ));
        let authorized = random::<f32>() > PAYMENT_REJECTED_PROBABILITY;

        let response = if authorized {
            SocketMessage::PaymentAuthorized(msg.customer_id, msg.amount, msg.restaurant_name)
        } else {
            SocketMessage::PaymentDenied(msg.customer_id, msg.amount, msg.restaurant_name)
        };
        self.logger
            .info(&format!("Response to authorization: {:?}", response));
        if let Err(e) = self.send_message(&response) {
            self.logger.error(&e.to_string());
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ExecutePayment {
    pub customer_id: u32,
    pub amount: f64,
}

#[async_handler]
impl Handler<ExecutePayment> for PaymentSystem {
    type Result = ();

    async fn handle(&mut self, msg: ExecutePayment, _ctx: &mut Self::Context) {
        self.logger.info("Executing payment...");
        sleep(std::time::Duration::from_secs(PAYMENT_DURATION)).await;
        let response = SocketMessage::PaymentExecuted(msg.customer_id, msg.amount);
        self.logger.info(&format!(
            "Payment executed for client {} with amount {}",
            msg.customer_id, msg.amount
        ));
        if let Err(e) = self.send_message(&response) {
            self.logger.error(&e.to_string());
        }
    }
}

#[async_handler]
impl Handler<InfoForUdpGatewayRequest> for PaymentSystem {
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
impl Handler<SetupReconnection> for PaymentSystem {
    type Result = ();

    async fn handle(&mut self, msg: SetupReconnection, _ctx: &mut Self::Context) -> Self::Result {
        let read_half = msg.read_half;
        let tcp_sender = msg.tcp_sender;

        PaymentSystem::add_stream(LinesStream::new(BufReader::new(read_half).lines()), _ctx);
        self.tcp_sender = tcp_sender;

        self.logger.info("Reconnection established")
    }
}

impl Handler<ReconnectToNewPedidosRust> for PaymentSystem {
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

impl PaymentSystem {
    pub async fn new(logger: Logger) -> Result<Addr<PaymentSystem>, Box<dyn std::error::Error>> {
        logger.info("Starting...");

        // Setting up ports
        let config = Configuration::new()?;

        let my_port = DEFAULT_PAYMENT_PORT;
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
        let payment_system = PaymentSystem::create(|ctx| {
            let (read_half, write_half) = split(stream);

            PaymentSystem::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);

            let tcp_sender = TcpSender {
                write_stream: Some(write_half),
            }
            .start();

            logger.debug("Created Payment System");

            let tcp_connector_actor = TcpConnector::create(|_tcp_ctx| TcpConnector {
                logger: Logger::new(Some("[TCP-CONNECTOR]")),
                source_port: my_port,
                dest_ports: dest_ports.clone(),
            });

            logger.info(&format!("Gateway started at port {}", my_port));
            PaymentSystem {
                tcp_sender,
                logger: Logger::new(Some("[PAYMENT-SYSTEM]")),
                config: config.clone(),
                my_port,
                peer_port: peer_port as u32,
                tcp_connector: tcp_connector_actor,
                udp_socket,
            }
        });
        Ok(payment_system)
    }

    #[allow(unreachable_patterns)]
    fn dispatch_message(&mut self, line_read: String, ctx: &mut <PaymentSystem as Actor>::Context) {
        let parsed_line = serde_json::from_str(&line_read);
        match parsed_line {
            Ok(message) => match message {
                SocketMessage::AuthorizePayment(customer_id, amount, restaurant_name) => {
                    ctx.address().do_send(ExecuteAutorization {
                        customer_id,
                        amount,
                        restaurant_name,
                    });
                }
                SocketMessage::ExecutePayment(customer_id, amount) => {
                    ctx.address().do_send(ExecutePayment {
                        customer_id,
                        amount,
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

impl StreamHandler<Result<String, io::Error>> for PaymentSystem {
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
