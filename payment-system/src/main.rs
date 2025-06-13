use common::constants::{
    DEFAULT_PR_HOST, DEFAULT_PR_PORT, PAYMENT_DURATION, PAYMENT_REJECTED_PROBABILITY,
};
use common::protocol::SocketMessage;
use common::tcp::tcp_message::TcpMessage;
use common::utils::logger::Logger;
use rand::random;
use std::{error::Error, sync::Arc};
use tokio::io::split;
use tokio::net::TcpStream;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::Mutex,
};

async fn execute_authorization(
    writer: Arc<Mutex<tokio::io::WriteHalf<TcpStream>>>,
    logger: Logger,
    client_id: u32,
    amount: f64,
    restaurant_name: String,
) -> Result<(), String> {
    logger.info("Executing authorization...");
    let authorized = random::<f32>() > PAYMENT_REJECTED_PROBABILITY;

    let response = if authorized {
        SocketMessage::PaymentAuthorized(client_id, amount, restaurant_name)
    } else {
        SocketMessage::PaymentDenied(client_id, amount, restaurant_name)
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
// TODO: QUE LE ENVIE UN MENSAJE DE REGISTRARSE PRIMERO AL PR
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

    // Enviar mensaje de registro al PR
    let register_msg = SocketMessage::RegisterPaymentSystem;
    let tcp_message = TcpMessage::from_serialized_json(&register_msg)?;
    {
        let mut writer = writer.lock().await;
        writer
            .write_all(tcp_message.data.as_bytes())
            .await
            .map_err(|e| format!("Failed to send register message: {}", e))?;
    }

    let reader = BufReader::new(reader_half);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();

        let parsed: Result<SocketMessage, _> = serde_json::from_str(trimmed);
        match parsed {
            Ok(SocketMessage::AuthorizePayment(client_id, amount, restaurant_name)) => {
                let writer_clone = Arc::clone(&writer);
                let logger = Logger::new(Some("[PAYMENT-SYSTEM]"));
                tokio::spawn(async move {
                    if let Err(e) = execute_authorization(
                        writer_clone,
                        logger.clone(),
                        client_id,
                        amount,
                        restaurant_name,
                    )
                    .await
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
