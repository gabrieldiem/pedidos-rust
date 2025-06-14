use crate::server::Server;
use common::utils::logger::Logger;
use std::{env, process};

mod client_connection;
mod connection_gateway;
mod connection_manager;
mod heartbeat;
mod messages;
mod nearby_entitys;
mod server;
mod server_peer;

fn parse_args() -> u32 {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Wrong program call. Usage: {} <id>", args[0]);
        process::exit(1);
    }

    match args[1].parse::<u32>() {
        Ok(id) => id,
        Err(_) => {
            eprintln!("Error: <id> must be a positive integer.");
            process::exit(1);
        }
    }
}

#[actix_rt::main]
async fn main() {
    let id = parse_args();
    let logger_prefix = format!("[PEDIDOS-RUST-{}]", id);
    let logger = Logger::new(Some(&logger_prefix));

    match Server::new(id, logger.clone()).await {
        Ok(server) => {
            if let Err(e) = server.run().await {
                logger.error(&format!("Server failed: {e}"));
            }
        }
        Err(error) => {
            logger.error(&format!("Server failed to start {error}"));
        }
    }
}
