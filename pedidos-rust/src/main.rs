use common::utils::logger::Logger;

use actix::{Actor, Addr, Context, StreamHandler};
use common::constants::DEFAULT_HOST;
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

impl StreamHandler<Result<String, std::io::Error>> for ClientConnection {
    fn handle(&mut self, message: Result<String, std::io::Error>, _ctx: &mut Self::Context) {
        match message {
            Ok(line) => {
                self.logger.debug(&format!("Received: '{}'", line));
                let seq_num = line
                    .split("=")
                    .collect::<Vec<&str>>()
                    .last()
                    .unwrap()
                    .to_string();
                let response = TcpMessage(format!("I (server) processed your seq={}\n", seq_num));

                if let Err(err) = self.tcp_sender.try_send(response) {
                    self.logger
                        .error(&format!("Failed to send response: {}", err));
                }
            }
            Err(err) => {
                self.logger.error(&format!("Error reading: {:?}", err));
            }
        }
    }
}

#[actix_rt::main]
async fn main() {
    let logger = Logger::new(Some("[PEDIDOS-RUST]"));
    logger.info("Starting...");

    let sockaddr_str = format!("{}:{}", DEFAULT_HOST, 7700);
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
