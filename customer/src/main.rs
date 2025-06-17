use crate::customer::{Customer, Start};
use actix::Addr;
use common::udp_gateway::{InfoForUdpGatewayRequest, UdpGateway};
use common::utils::logger::Logger;
use std::{env, process};
use tokio::spawn;

mod customer;

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

async fn run(customer: Addr<Customer>, logger: Logger) -> std::io::Result<()> {
    match customer.send(Start).await {
        Ok(_) => {}
        Err(e) => eprintln!("Could not start actor: {e}"),
    }

    spawn(async move {
        let data_res = customer.send(InfoForUdpGatewayRequest {}).await;
        match data_res {
            Ok(data) => {
                if let Err(e) = UdpGateway::run::<Customer>(
                    data.port,
                    logger.clone(),
                    customer,
                    data.configuration,
                    data.udp_socket,
                )
                .await
                {
                    logger.error(&format!("Connection gateway error during loop: {}", e));
                }
            }
            Err(e) => eprintln!("Could not start UdpGateway: {e}"),
        }
    });

    actix_rt::signal::ctrl_c().await
}

#[actix_rt::main]
async fn main() {
    let id = parse_args();
    let logger = Logger::new(Some("[CUSTOMER]"));

    match Customer::new(id, logger.clone()).await {
        Ok(customer) => {
            if let Err(error) = run(customer, logger.clone()).await {
                logger.error(&format!("Customer failed: {error}"));
            }
        }
        Err(error) => {
            logger.error(&format!("Customer failed to start: {error}"));
        }
    }
}
