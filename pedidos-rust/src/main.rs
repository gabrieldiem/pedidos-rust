use crate::server::Server;
use std::{env, process};

mod client_connection;
mod connection_manager;
mod heartbeat;
mod messages;
mod nearby_entitys;
mod server;

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
    match Server::new(id) {
        Ok(server) => {
            server.start().await;
        }
        Err(error) => {
            eprintln!("Server failed to start");
            eprintln!("{}", error);
        }
    }
}
