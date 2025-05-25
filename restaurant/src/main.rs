mod cliente_restaurante_de_juguete;

use common::utils::logger::Logger;
use rand::random;
use std::{error::Error, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    sync::Mutex,
    time::sleep,
};

//TODO: implementar un protocolo entre pedidos rust y restaurente. Esto sigue siendo de juguete
async fn handle_order(
    order: String,
    writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    logger: Logger,
) -> Result<(), Box<dyn Error>> {
    logger.info(&format!("Order received: {}", order));

    let accepted = random::<f32>() > 0.1;

    if accepted {
        {
            let mut writer = writer.lock().await;
            writer.write_all(b"OK\n").await?;
        }

        logger.info(&format!("Order accepted: {}", order));
        sleep(Duration::from_secs(2)).await;
        logger.info(&format!("Order {} is ready", order));

        let respuesta = format!("Order '{}' is ready\n", order);
        {
            let mut writer = writer.lock().await;
            writer.write_all(respuesta.as_bytes()).await?;
        }
    } else {
        let mut writer = writer.lock().await;
        writer.write_all(b"REJECTED\n").await?;
        logger.info(&format!("Order rejected due to lack os stock: {}", order));
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let logger = Logger::new(Some("[RESTAURANT]"));
    logger.info("Starting...");

    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    logger.info("Restaurante listening on 127.0.0.1:8080..");

    let (socket, _) = listener.accept().await?;
    logger.info(&format!(
        "Connection established with {}",
        socket.peer_addr()?
    ));

    let (reader, writer) = socket.into_split();
    let reader = BufReader::new(reader);
    let writer = Arc::new(Mutex::new(writer));
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
