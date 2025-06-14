use common::constants::{
    DEFAULT_PR_HOST, DEFAULT_PR_PORT, MAX_ORDER_DURATION, MIN_ORDER_DURATION,
    ORDER_REJECTED_PROBABILITY,
};
use common::protocol::{Location, SocketMessage};
use common::tcp::tcp_message::TcpMessage;
use common::utils::logger::Logger;
use rand::{Rng, random};
use std::{env, error::Error, sync::Arc, time::Duration};
use tokio::net::TcpStream;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, split},
    sync::Mutex,
};

struct Restaurant {
    location: Location,
    name: String,
}

/// TODO: que conteste al pinger
async fn handle_order(
    writer: Arc<Mutex<tokio::io::WriteHalf<TcpStream>>>,
    logger: Logger,
    client_id: u32,
    price: f64,
    restaurant_location: Location,
) -> Result<(), Box<dyn Error>> {
    logger.info(&format!(
        "Order received from client {} with price {}",
        client_id, price
    ));

    // Answering PedidosRust that the order is in progress
    let response = SocketMessage::OrderInProgress(client_id);
    let tcp_message = TcpMessage::from_serialized_json(&response)?;

    {
        let mut writer_guard = writer.lock().await;
        writer_guard
            .write_all(tcp_message.data.as_bytes())
            .await
            .map_err(|e| format!("Failed to send message: {}", e))?;
    }

    logger.info(&format!(
        "Orden from client {} with price {} is in progress...",
        client_id, price
    ));
    let secs = rand::rng().random_range(MIN_ORDER_DURATION..=MAX_ORDER_DURATION);
    tokio::time::sleep(Duration::from_secs(secs)).await;

    // Simulating stock availability for accepting or rejecting the order
    let accepted = random::<f32>() > ORDER_REJECTED_PROBABILITY;

    if !accepted {
        logger.info(&format!(
            "Order from client {} with price {} rejected due to lack of stock",
            client_id, price
        ));
        let response = SocketMessage::OrderCalcelled(client_id);
        let tcp_message = TcpMessage::from_serialized_json(&response)?;
        let mut writer_guard = writer.lock().await;
        writer_guard
            .write_all(tcp_message.data.as_bytes())
            .await
            .map_err(|e| format!("Failed to send message: {}", e))?;
        return Ok(());
    }

    logger.info(&format!(
        "Order from client {} with price {} is ready",
        client_id, price
    ));
    let response = SocketMessage::OrderReady(client_id, restaurant_location);
    let tcp_message = TcpMessage::from_serialized_json(&response)?;
    let mut writer_guard = writer.lock().await;
    writer_guard
        .write_all(tcp_message.data.as_bytes())
        .await
        .map_err(|e| format!("Failed to send message: {}", e))?;
    Ok(())
}

async fn inform_location(
    writer: &Arc<Mutex<tokio::io::WriteHalf<TcpStream>>>,
    logger: &Logger,
    restaurant: &Restaurant,
) -> Result<(), Box<dyn Error>> {
    let location_msg = SocketMessage::InformLocation(restaurant.location, restaurant.name.clone());
    let tcp_message = TcpMessage::from_serialized_json(&location_msg)?;

    let mut writer_guard = writer.lock().await;
    writer_guard
        .write_all(tcp_message.data.as_bytes())
        .await
        .map_err(|e| format!("Failed to send InformLocation: {}", e))?;

    logger.info(&format!(
        "Informed location '{}' at ({}, {}) to PedidosRust",
        restaurant.name, restaurant.location.x, restaurant.location.y
    ));

    Ok(())
}

/// Parses the restaurant's information from the command line arguments.
///
/// Expects three arguments in the following order:
/// 1. Restaurant name (`String`)
/// 2. X coordinate (`u16`)
/// 3. Y coordinate (`u16`)
///
/// # Errors
///
/// Returns an error if any argument is missing or if the coordinates cannot be parsed as `u16`.
///
/// # Returns
///
/// A `Restaurant` struct with the provided name and location.
fn parse_restaurant_from_args() -> Result<Restaurant, Box<dyn Error>> {
    let mut args = env::args().skip(1); // Skip program name

    let name = args.next().ok_or("Missing restaurant name")?;
    let x_str = args.next().ok_or("Missing x coordinate")?;
    let y_str = args.next().ok_or("Missing y coordinate")?;

    let x: u16 = x_str
        .parse()
        .map_err(|_| format!("Invalid x coordinate: {}", x_str))?;
    let y: u16 = y_str
        .parse()
        .map_err(|_| format!("Invalid y coordinate: {}", y_str))?;

    Ok(Restaurant {
        name,
        location: Location::new(x, y),
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let logger = Logger::new(Some("[RESTAURANT]"));
    logger.info("Starting...");

    let restaurant = Arc::new(parse_restaurant_from_args()?);

    let server_sockeaddr_str = format!("{}:{}", DEFAULT_PR_HOST, DEFAULT_PR_PORT);
    let stream = TcpStream::connect(server_sockeaddr_str.clone()).await?;

    logger.info(&format!("Using address {}", stream.local_addr()?));
    logger.info(&format!("Connected to server {}", server_sockeaddr_str));

    let (reader_half, writer_half) = split(stream);
    let writer = Arc::new(Mutex::new(writer_half));

    inform_location(&writer, &logger, &restaurant).await?;

    let reader = BufReader::new(reader_half);
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim().to_string();
        let order: Result<SocketMessage, _> = serde_json::from_str(&trimmed);
        match order {
            Ok(SocketMessage::PrepareOrder(client_id, price)) => {
                let writer_clone = Arc::clone(&writer);
                let logger = Logger::new(Some("[RESTAURANT]"));
                let restaurant_location = restaurant.location;
                tokio::spawn(async move {
                    if let Err(e) = handle_order(
                        writer_clone,
                        logger.clone(),
                        client_id,
                        price,
                        restaurant_location,
                    )
                    .await
                    {
                        logger.error(&format!(
                            "Error preparing order for client {}: {}",
                            client_id, e
                        ));
                    }
                });
            }
            Ok(_) => {
                logger.error(&format!("Unexpected message: {}", trimmed));
                continue;
            }
            Err(e) => {
                logger.error(&format!("Failed to parse order: {}", e));
                continue;
            }
        }
    }

    println!("Connection closed by server");
    Ok(())
}
