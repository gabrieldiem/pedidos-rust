use crate::rider::{Rider, Start};
use actix::Addr;
use common::utils::logger::Logger;
use std::{env, process};

mod rider;

fn parse_args() -> u32 {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Wrong program call. Usage is: {} <id>", args[0]);
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

async fn run(rider: Addr<Rider>) -> std::io::Result<()> {
    match rider.send(Start).await {
        Ok(_) => {}
        Err(e) => eprintln!("Could not start actor: {e}"),
    }

    actix_rt::signal::ctrl_c().await
}

#[actix_rt::main]
async fn main() {
    let id = parse_args();
    let logger = Logger::new(Some("[RIDER]"));

    match Rider::new(id, logger.clone()).await {
        Ok(rider) => {
            if let Err(error) = run(rider).await {
                logger.error(&format!("Rider failed: {error}"));
            }
        }
        Err(error) => {
            logger.error(&format!("Rider failed to start: {error}"));
        }
    }
}
