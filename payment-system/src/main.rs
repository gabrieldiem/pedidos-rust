use common::constants::{
    DEFAULT_PR_HOST, DEFAULT_PR_PORT, PAYMENT_DURATION, PAYMENT_REJECTED_PROBABILITY,
};
use common::protocol::SocketMessage;
use common::tcp::tcp_message::TcpMessage;
use common::utils::logger::Logger;
use rand::random;
use std::{error::Error, sync::Arc};
use tokio::net::TcpStream;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, split},
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

    let msg_to_send = serde_json::to_string(&response)
        .map_err(|e| format!("Failed to serialize message: {}", e))?;

    let tcp_message = TcpMessage {
        data: msg_to_send + "\n",
    };

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
    let msg_to_send = serde_json::to_string(&response)
        .map_err(|e| format!("Failed to serialize message: {}", e))?;

    logger.info(&format!(
        "Payment executed for client {} with amount {}",
        client_id, amount
    ));

    let tcp_message = TcpMessage {
        data: msg_to_send + "\n",
    };

    let mut writer = writer.lock().await;
    writer
        .write_all(tcp_message.data.as_bytes())
        .await
        .map_err(|e| format!("Failed to send message: {}", e))?;

    Ok(())
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
        let trimmed = line.trim();

        let parsed: Result<SocketMessage, _> = serde_json::from_str(trimmed);
        match parsed {
            Ok(SocketMessage::AuthorizePayment(client_id, amount)) => {
                let writer_clone = Arc::clone(&writer);
                let logger = Logger::new(Some("[PAYMENT-SYSTEM]"));
                tokio::spawn(async move {
                    if let Err(e) =
                        execute_authorization(writer_clone, logger.clone(), client_id, amount).await
                    {
                        logger.error(&format!(
                            "Error executing authorization for client {}: {}",
                            client_id, e
                        ));
                    }
                });
            }
            Ok(SocketMessage::ExecutePayment(client_id, amount)) => {
                let writer_clone = Arc::clone(&writer);
                let logger = Logger::new(Some("[PAYMENT-SYSTEM]"));
                tokio::spawn(async move {
                    if let Err(e) =
                        execute_payment(writer_clone, logger.clone(), client_id, amount).await
                    {
                        logger.error(&format!(
                            "Error executing payment for client {}: {}",
                            client_id, e
                        ));
                    }
                });
            }
            _ => {
                let logger = Logger::new(Some("[PAYMENT-SYSTEM]"));
                logger.error(&format!("UNKNOWN OR MALFORMED MESSAGE: {}", trimmed));
            }
        }
    }

    println!("Connection closed by server");
    Ok(())
}
