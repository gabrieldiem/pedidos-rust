use common::constants::{
    DEFAULT_PR_HOST, DEFAULT_PR_PORT, PAYMENT_DURATION, PAYMENT_REJECTED_PROBABILITY,
};
use common::utils::logger::Logger;
use rand::random;
use std::{error::Error, sync::Arc};
use tokio::net::TcpStream;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, split},
    sync::Mutex,
};

//TODO: implementar un protocolo entre pedidos rust y el sistema de pagos
async fn execute_authorization(
    writer: Arc<Mutex<tokio::io::WriteHalf<TcpStream>>>,
    logger: Logger,
) {
    logger.info("Executing authorizarion...");
    let authorized = random::<f32>() > PAYMENT_REJECTED_PROBABILITY;
    let response = if authorized {
        "PAYMENT AUTHORIZED\n"
    } else {
        "PAYMENT REJECTED\n"
    };
    logger.info(&format!("Response: {}", response.trim()));

    let mut writer = writer.lock().await;
    let _ = writer.write_all(response.as_bytes()).await;
}

async fn execute_payment(writer: Arc<Mutex<tokio::io::WriteHalf<TcpStream>>>, logger: Logger) {
    logger.info("Executing payment...");
    tokio::time::sleep(std::time::Duration::from_secs(PAYMENT_DURATION)).await;

    let response = "PAYMENT EXECUTED\n";
    logger.info(&format!("Response: {}", response.trim()));

    let mut writer = writer.lock().await;
    let _ = writer.write_all(response.as_bytes()).await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let logger = Logger::new(Some("[PAYMENT-SYSTEM]"));
    logger.info("Starting...");

    let server_sockeaddr_str = format!("{}:{}", DEFAULT_PR_HOST, DEFAULT_PR_PORT);
    let stream = TcpStream::connect(server_sockeaddr_str.clone()).await?;

    logger.info(&format!("Using address {}", stream.local_addr()?));
    logger.info(&format!("Connected to server {}", server_sockeaddr_str));

    let (reader_half, writer_half) = split(stream);
    let writer = Arc::new(Mutex::new(writer_half));
    let reader = BufReader::new(reader_half);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let request = line.trim().to_string();
        let writer_clone = Arc::clone(&writer);
        let logger = Logger::new(Some("[PAYMENT-SYSTEM]"));

        let upper_request = request.to_uppercase();
        if upper_request.contains("AUTORIZE") {
            tokio::spawn(execute_authorization(writer_clone, logger));
        } else if upper_request.contains("EXECUTE") {
            tokio::spawn(execute_payment(writer_clone, logger));
        } else {
            let writer_clone = Arc::clone(&writer);
            tokio::spawn(async move {
                let mut writer = writer_clone.lock().await;
                let _ = writer.write_all(b"UNKNOWN COMMAND\n").await;
            });
        }
    }

    println!("Connection closed by server");
    Ok(())
}
