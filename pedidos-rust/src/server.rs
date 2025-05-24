use crate::client_connection::ClientConnection;

use crate::connection_manager::ConnectionManager;
use actix::{Actor, Addr, StreamHandler};
use common::constants::{DEFAULT_PR_HOST, DEFAULT_PR_PORT};
use common::tcp::tcp_sender::TcpSender;
use common::utils::logger::Logger;
use tokio::io::{AsyncBufReadExt, BufReader, split};
use tokio::net::TcpListener;
use tokio_stream::wrappers::LinesStream;

pub struct Server {
    logger: Logger,
    connection_manager: Addr<ConnectionManager>,
}

impl Server {
    pub fn new() -> Server {
        let logger = Logger::new(Some("[PEDIDOS-RUST]"));

        let connection_manager = ConnectionManager::create(|_ctx| ConnectionManager::new());

        Server {
            logger,
            connection_manager,
        }
    }

    pub async fn start(&self) {
        let sockaddr_str = format!("{}:{}", DEFAULT_PR_HOST, DEFAULT_PR_PORT);
        let listener = TcpListener::bind(sockaddr_str.clone()).await.unwrap();

        self.logger.info(&format!(
            "Listening for connections on {}",
            sockaddr_str.clone()
        ));

        while let Ok((stream, client_sockaddr)) = listener.accept().await {
            self.logger
                .info(&format!("Client connected: {client_sockaddr}"));

            ClientConnection::create(|ctx| {
                self.logger.debug("Created ClientConnection");
                let (read_half, write_half) = split(stream);

                ClientConnection::add_stream(
                    LinesStream::new(BufReader::new(read_half).lines()),
                    ctx,
                );
                let tcp_sender = TcpSender {
                    write_stream: Some(write_half),
                }
                .start();

                let port = client_sockaddr.port() as u32;
                ClientConnection {
                    tcp_sender,
                    logger: Logger::new(Some(&format!("[PEDIDOS-RUST] [CONN:{}]", &port))),
                    id: port,
                    connection_manager: self.connection_manager.clone(),
                    peer_location: None,
                }
            });
        }
    }
}
