use common::constants::{DEFAULT_PR_HOST, DEFAULT_PR_PORT};
use common::utils::logger::Logger;

use actix::{Actor, ActorContext, AsyncContext, Context, Handler};
use actix::{Addr, Message, StreamHandler};
use actix_async_handler::async_handler;
use common::protocol::{GetRestaurants, Restaurants, SocketMessage};
use common::tcp::tcp_message::TcpMessage;
use common::tcp::tcp_sender::TcpSender;
use std::io;
use tokio::io::{AsyncBufReadExt, BufReader, split};
use tokio::net::TcpStream;
use tokio_stream::wrappers::LinesStream;

struct Customer {
    tcp_sender: Addr<TcpSender>,
    logger: Logger,
}

impl Actor for Customer {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Start;

#[derive(Message)]
#[rtype(result = "()")]
pub struct Stop;

#[async_handler]
impl Handler<Start> for Customer {
    type Result = ();

    async fn handle(&mut self, _msg: Start, _ctx: &mut Self::Context) -> Self::Result {
        _ctx.address().do_send(GetRestaurants);
    }
}

#[async_handler]
impl Handler<Stop> for Customer {
    type Result = ();

    async fn handle(&mut self, _msg: Stop, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.info("Stopping Customer actor");
        _ctx.stop();
    }
}

#[async_handler]
impl Handler<GetRestaurants> for Customer {
    type Result = ();

    async fn handle(&mut self, _msg: GetRestaurants, _ctx: &mut Self::Context) -> Self::Result {
        self.logger.debug("Getting Restaurants");

        // message serialization
        let msg_to_send = match serde_json::to_string(&SocketMessage::GetRestaurants) {
            Ok(ok_result) => ok_result,
            Err(e) => {
                self.logger
                    .error(&format!("Failed to serialize message: {}", e));
                return;
            }
        };

        // sending message
        if let Err(e) = self.tcp_sender.try_send(TcpMessage(msg_to_send + "\n")) {
            self.logger
                .error(&format!("Failed to write to stream: {}", e));
            return;
        }
    }
}

#[async_handler]
impl Handler<Restaurants> for Customer {
    type Result = ();

    async fn handle(&mut self, msg: Restaurants, _ctx: &mut Self::Context) -> Self::Result {
        self.logger
            .debug(&format!("These are the restaurants: {}", msg.0));
    }
}

impl Customer {
    #[allow(unreachable_patterns)]
    fn dispatch_message(&mut self, line_read: String, ctx: &mut <Customer as Actor>::Context) {
        let parsed_line = serde_json::from_str(&line_read);
        match parsed_line {
            Ok(message) => match message {
                SocketMessage::GetRestaurants => {
                    ctx.address().do_send(GetRestaurants);
                }
                SocketMessage::Restaurants(data) => {
                    ctx.address().do_send(Restaurants(data));
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
}

#[actix_rt::main]
async fn main() -> io::Result<()> {
    let logger = Logger::new(Some("[CUSTOMER]"));
    logger.info("Starting...");

    let server_sockeaddr_str = format!("{}:{}", DEFAULT_PR_HOST, DEFAULT_PR_PORT);
    let stream = TcpStream::connect(server_sockeaddr_str.clone()).await?;

    logger.info(&format!("Using address {}", stream.local_addr()?));
    logger.info(&format!("Connected to server {}", server_sockeaddr_str));

    let customer = Customer::create(|ctx| {
        let (read_half, write_half) = split(stream);

        Customer::add_stream(LinesStream::new(BufReader::new(read_half).lines()), ctx);
        let tcp_sender = TcpSender {
            write_stream: Some(write_half),
        }
        .start();

        logger.debug("Created CustomerGateway");
        Customer {
            tcp_sender,
            logger: Logger::new(Some("[CUSTOMER]")),
        }
    });

    match customer.send(Start).await {
        Ok(_) => {}
        Err(e) => logger.error(&format!("Could not start actor: {e}")),
    }

    actix_rt::signal::ctrl_c().await?;

    Ok(())
}
