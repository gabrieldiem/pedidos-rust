use common::constants::{DEFAULT_PR_HOST, DEFAULT_PR_PORT, PAYMENT_REJECTED_PROBABILITY};
use common::utils::logger::Logger;
use rand::random;
use std::{error::Error, sync::Arc};
use tokio::net::TcpStream;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, split},
    sync::Mutex,
};

//TODO: implementar un protocolo para el payment system
async fn handle_request(
    order: String,
    writer: Arc<Mutex<tokio::io::WriteHalf<TcpStream>>>,
    logger: Logger,
) -> Result<(), Box<dyn Error>> {
    logger.info(&format!("Request received: {}", order));

    if order.to_uppercase().contains("AUTORIZAR") {
        let writer_clone = Arc::clone(&writer);
        let logger_clone = logger.clone();
        tokio::spawn(async move {
            let autorizado = random::<f32>() > PAYMENT_REJECTED_PROBABILITY;
            let respuesta = if autorizado {
                "PAGO AUTORIZADO\n"
            } else {
                "PAGO RECHAZADO\n"
            };
            logger_clone.info(&format!("Respuesta: {}", respuesta.trim()));
            let mut writer = writer_clone.lock().await;
            let _ = writer.write_all(respuesta.as_bytes()).await;
        });
    } else if order.to_uppercase().contains("EJECUTAR") {
        let writer_clone = Arc::clone(&writer);
        let logger_clone = logger.clone();
        tokio::spawn(async move {
            logger_clone.info("Ejecutando pago...");
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let respuesta = "PAGO EJECUTADO\n";
            logger_clone.info("Respuesta: PAGO EJECUTADO");
            let mut writer = writer_clone.lock().await;
            let _ = writer.write_all(respuesta.as_bytes()).await;
        });
    } else {
        let mut writer = writer.lock().await;
        writer.write_all(b"COMANDO DESCONOCIDO\n").await?;
    }

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
        let request = line.trim().to_string();
        let writer_clone = Arc::clone(&writer);

        tokio::spawn(async move {
            if let Err(e) =
                handle_request(request, writer_clone, Logger::new(Some("[PAYMENT-SYSTEM]"))).await
            {
                eprintln!("Error handling request: {:?}", e);
            }
        });
    }

    println!("Connection closed by server");
    Ok(())
}
