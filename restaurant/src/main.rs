use common::constants::{DEFAULT_PR_HOST, DEFAULT_PR_PORT, MAX_ORDER_DURATION, MIN_ORDER_DURATION};
use common::utils::logger::Logger;
use rand::{Rng, random};
use std::{error::Error, sync::Arc, time::Duration};
use tokio::net::TcpStream;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, split},
    sync::Mutex,
};

//TODO: implementar un protocolo entre pedidos rust y restaurente. Esto sigue siendo de juguete
async fn handle_order(
    order: String,
    writer: Arc<Mutex<tokio::io::WriteHalf<TcpStream>>>,
    logger: Logger,
) -> Result<(), Box<dyn Error>> {
    logger.info(&format!("Order received: {}", order));

    {
        let mut writer = writer.lock().await;
        writer.write_all(b"PREPARING\n").await?;
    }

    logger.info(&format!("Preparing order: {}...", order));
    let secs = rand::rng().random_range(MIN_ORDER_DURATION..=MAX_ORDER_DURATION);
    tokio::time::sleep(Duration::from_secs(secs)).await;

    let accepted = random::<f32>() > 0.1;

    if !accepted {
        logger.info(&format!("Order {} rejected due to lack of stock", order));
        let mut writer = writer.lock().await;
        writer.write_all(b"REJECTED\n").await?;
        return Ok(());
    }

    logger.info(&format!("Order {} is ready", order));
    let respuesta = format!("Order '{}' is ready\n", order);
    {
        let mut writer = writer.lock().await;
        writer.write_all(respuesta.as_bytes()).await?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let logger = Logger::new(Some("[RESTAURANT]"));
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
        let order = line.trim().to_string();
        let writer_clone = Arc::clone(&writer);

        tokio::spawn(async move {
            if let Err(e) =
                handle_order(order, writer_clone, Logger::new(Some("[RESTAURANT]"))).await
            {
                eprintln!("Error handling order: {:?}", e);
            }
        });
    }

    println!("Connection closed by client");
    Ok(())
}
