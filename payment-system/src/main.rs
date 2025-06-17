use crate::payment_system::{PaymentSystem, Start};
use actix::Addr;
use common::utils::logger::Logger;

mod payment_system;

async fn run(payment_system: Addr<PaymentSystem>) -> std::io::Result<()> {
    match payment_system.send(Start).await {
        Ok(_) => {}
        Err(e) => eprintln!("Could not start actor: {e}"),
    }

    actix_rt::signal::ctrl_c().await
}

#[actix_rt::main]
async fn main() {
    let logger = Logger::new(Some("[PAYMENT-SYSTEM]"));

    match PaymentSystem::new(logger.clone()).await {
        Ok(payment_system) => {
            if let Err(error) = run(payment_system).await {
                logger.error(&format!("PaymentSystem failed: {error}"));
            }
        }
        Err(error) => {
            logger.error(&format!("PaymentSystem failed to start: {error}"));
        }
    }
}
