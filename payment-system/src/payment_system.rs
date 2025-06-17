use actix::{Actor, ActorContext, AsyncContext, Context, Handler};
use actix::{Addr, Message, StreamHandler};
use actix_async_handler::async_handler;
use common::configuration::Configuration;
use common::constants::{DEFAULT_PAYMENT_PORT, PAYMENT_DURATION, PAYMENT_REJECTED_PROBABILITY};
use common::protocol::{SocketMessage, Stop};
use common::tcp::tcp_connector::TcpConnector;
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use rand::random;
use std::io;
use tokio::io::{AsyncBufReadExt, BufReader, split};
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

        let msg = SocketMessage::RegisterPaymentSystem;
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
}
