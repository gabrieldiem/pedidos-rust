use common::constants::{
    DEFAULT_PAYMENT_HOST, DEFAULT_PAYMENT_PORT, PAYMENT_DURATION, PAYMENT_REJECTED_PROBABILITY,
};
use common::protocol::SocketMessage;
use common::tcp::tcp_message::TcpMessage;
use common::utils::logger::Logger;
use rand::random;
use std::{error::Error, sync::Arc};
use tokio::net::{TcpListener, TcpStream};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
};

async fn execute_authorization(
    writer: Arc<Mutex<tokio::io::WriteHalf<TcpStream>>>,
    logger: Logger,
    client_id: u32,
    amount: f64,
) -> Result<(), String> {
    logger.info("Executing authorization...");
    let authorized = random::<f32>() > PAYMENT_REJECTED_PROBABILITY;

    let response = if authorized {
        SocketMessage::PaymentAuthorized(client_id, amount)
    } else {
        SocketMessage::PaymentDenied(client_id, amount)
    };
    logger.info(&format!("Response to authorization: {:?}", response));

    let tcp_message = TcpMessage::from_serialized_json(&response)?;

    let mut writer = writer.lock().await;
    writer
        .write_all(tcp_message.data.as_bytes())
        .await
        .map_err(|e| format!("Failed to send message: {}", e))?;

    Ok(())
}

async fn execute_payment(
    writer: Arc<Mutex<tokio::io::WriteHalf<TcpStream>>>,
    logger: Logger,
    client_id: u32,
    amount: f64,
) -> Result<(), String> {
    logger.info("Executing payment...");
    tokio::time::sleep(std::time::Duration::from_secs(PAYMENT_DURATION)).await;

    let response = SocketMessage::PaymentExecuted(client_id, amount);
    let tcp_message = TcpMessage::from_serialized_json(&response)?;

    logger.info(&format!(
        "Payment executed for client {} with amount {}",
        client_id, amount
    ));

    let mut writer = writer.lock().await;
    writer
        .write_all(tcp_message.data.as_bytes())
        .await
        .map_err(|e| format!("Failed to send message: {}", e))?;

    Ok(())
}

async fn handle_client(stream: TcpStream, logger: Logger) {
    let (reader_half, writer_half) = tokio::io::split(stream);
    let writer = Arc::new(Mutex::new(writer_half));
    let reader = BufReader::new(reader_half);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();

        match serde_json::from_str::<SocketMessage>(trimmed) {
            Ok(SocketMessage::AuthorizePayment(client_id, amount)) => {
                let writer_clone = Arc::clone(&writer);
                let logger = logger.clone();
                tokio::spawn(async move {
                    if let Err(e) =
                        execute_authorization(writer_clone, logger.clone(), client_id, amount).await
                    {
                        logger.error(&format!("Error in authorization: {}", e));
                    }
                });
            }
            Ok(SocketMessage::ExecutePayment(client_id, amount)) => {
                let writer_clone = Arc::clone(&writer);
                let logger = logger.clone();
                tokio::spawn(async move {
                    if let Err(e) =
                        execute_payment(writer_clone, logger.clone(), client_id, amount).await
                    {
                        logger.error(&format!("Error in payment: {}", e));
                    }
                });
            }
            _ => {
                logger.error(&format!("UNKNOWN OR MALFORMED MESSAGE: {}", trimmed));
            }
        }
    }

    logger.info("Client disconnected.");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let logger = Logger::new(Some("[PAYMENT-SYSTEM]"));
    logger.info("Starting Payment Gateway server...");

    let addr = format!("{}:{}", DEFAULT_PAYMENT_HOST, DEFAULT_PAYMENT_PORT);
    let listener = TcpListener::bind(addr.clone()).await?;
    logger.info(&format!("Listening on {}", addr));

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                logger.info(&format!("New client connected: {}", addr));
                let logger_clone = logger.clone();
                tokio::spawn(handle_client(stream, logger_clone));
            }
            Err(e) => {
                logger.error(&format!("Failed to accept connection: {}", e));
            }
        }
    }
}
