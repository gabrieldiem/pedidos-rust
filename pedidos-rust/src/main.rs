use crate::server::Server;

mod client_connection;
mod connection_manager;
mod messages;
mod server;

#[actix_rt::main]
async fn main() {
    let server = Server::new();
    server.start().await;
}
