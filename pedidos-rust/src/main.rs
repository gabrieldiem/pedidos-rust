use common::utils::logger::Logger;
use std::io;

use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, StreamHandler};
use actix_async_handler::async_handler;
use common::constants::*;
use common::protocol::{Order, SocketMessage};
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, BufReader, split};
use tokio::net::TcpListener;
use tokio_stream::wrappers::LinesStream;

struct ClientConnection {
    _client_sockaddr: SocketAddr,
    tcp_sender: Addr<TcpSender>,
    logger: Logger,
}

impl Actor for ClientConnection {
    type Context = Context<Self>;
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct SendRestaurants;

#[async_handler]
impl Handler<SendRestaurants> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, _msg: SendRestaurants, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Sending Restaurants");

        let restaurantes =
            serde_json::to_string(&vec!["McDonalds", "BurgerKing", "Wendys"]).unwrap();

        let msg = SocketMessage::Restaurants(restaurantes);

        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
        }
    }
}

#[async_handler]
impl Handler<Order> for ClientConnection {
    type Result = ();

    async fn handle(&mut self, msg: Order, _ctx: &mut Self::Context) -> Self::Result {
        let order = msg.0;
        self.logger.debug(&format!(
            "Processing order of {} from {}",
            order.amount, order.restaurant
        ));

        let msg = SocketMessage::PushNotification("Processing order".to_owned());
        if let Err(e) = self.send_message(&msg) {
            self.logger.error(&e.to_string());
            return;
        }
    }
}

impl ClientConnection {
    #[allow(unreachable_patterns)]
    fn dispatch_message(
        &mut self,
        line_read: String,
        ctx: &mut <ClientConnection as Actor>::Context,
    ) {
        let parsed_line = serde_json::from_str(&line_read);
        match parsed_line {
            Ok(message) => match message {
                SocketMessage::GetRestaurants => {
                    self.logger.debug("Got request for GetRestaurants");
                    ctx.address().do_send(SendRestaurants);
                }
                SocketMessage::Order(order) => {
                    self.logger.debug("Got order request");
                    ctx.address().do_send(Order(order));
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
        if let Err(e) = self.tcp_sender.try_send(TcpMessage(msg_to_send + "\n")) {
            return Err(format!("Failed to write to stream: {}", e));
        }

        Ok(())
    }
}

impl StreamHandler<Result<String, io::Error>> for ClientConnection {
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

#[actix_rt::main]
async fn main() {
    let logger = Logger::new(Some("[PEDIDOS-RUST]"));
    logger.info("Starting...");

    let sockaddr_str = format!("{}:{}", DEFAULT_PR_HOST, DEFAULT_PR_PORT);
    let listener = TcpListener::bind(sockaddr_str.clone()).await.unwrap();

    logger.info(&format!(
        "Listening for connections on {}",
        sockaddr_str.clone()
    ));

    while let Ok((stream, client_sockaddr)) = listener.accept().await {
        logger.info(&format!("Client connected: {client_sockaddr}"));

        ClientConnection::create(|ctx| {
            logger.debug("Created ClientConnection");
            let (read_half, write_half) = split(stream);

            ClientConnection::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);
            let tcp_sender = TcpSender {
                write_stream: Some(write_half),
            }
            .start();

            ClientConnection {
                _client_sockaddr: client_sockaddr,
                tcp_sender,
                logger: Logger::new(Some(&format!(
                    "[PEDIDOS-RUST] [CONN:{}]",
                    &client_sockaddr.port()
                ))),
            }
        });
    }
}
