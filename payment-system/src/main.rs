use crate::payment_system::{PaymentSystem, Start};
use actix::Addr;
use common::udp_gateway::{InfoForUdpGatewayRequest, UdpGateway};
use common::utils::logger::Logger;
use tokio::spawn;

mod payment_system;

async fn run(payment_system: Addr<PaymentSystem>, logger: Logger) -> std::io::Result<()> {
    match payment_system.send(Start).await {
        Ok(_) => {}
        Err(e) => eprintln!("Could not start actor: {e}"),
    }

    spawn(async move {
        let data_res = payment_system.send(InfoForUdpGatewayRequest {}).await;
        match data_res {
            Ok(data) => {
                if let Err(e) = UdpGateway::run::<PaymentSystem>(
                    data.port,
                    logger.clone(),
                    payment_system,
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
    let logger = Logger::new(Some("[PAYMENT-SYSTEM]"));

    match PaymentSystem::new(logger.clone()).await {
        Ok(payment_system) => {
            if let Err(error) = run(payment_system, logger.clone()).await {
                logger.error(&format!("PaymentSystem failed: {error}"));
            }
        }
        Err(error) => {
            logger.error(&format!("PaymentSystem failed to start: {error}"));
        }
    }
}
