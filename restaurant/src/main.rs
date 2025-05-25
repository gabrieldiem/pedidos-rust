mod cliente_restaurante_de_juguete;

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
    pedido: String,
    writer: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
) -> Result<(), Box<dyn Error>> {
    println!("Order received: {}", pedido);

    let accepted = random::<f32>() > 0.1;

    if accepted {
        {
            let mut writer = writer.lock().await;
            writer.write_all(b"OK\n").await?;
        }

        println!("Order accepted: {}", pedido);
        sleep(Duration::from_secs(2)).await;
        println!("Order is ready: {}", pedido);

        let respuesta = format!("Order '{}' is ready\n", pedido);
        {
            let mut writer = writer.lock().await;
            writer.write_all(respuesta.as_bytes()).await?;
        }
    } else {
        let mut writer = writer.lock().await;
        writer.write_all(b"REJECTED\n").await?;
        println!("Order rejected: {}", pedido);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Restaurante listening on 127.0.0.1:8080...");

    let (socket, _) = listener.accept().await?;
    println!("Connection established with {:?}", socket.peer_addr()?);

    let (reader, writer) = socket.into_split();
    let reader = BufReader::new(reader);
    let writer = Arc::new(Mutex::new(writer));
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let order = line.trim().to_string();
        let writer_clone = Arc::clone(&writer);

        tokio::spawn(async move {
            if let Err(e) = handle_order(order, writer_clone).await {
                eprintln!("Error al manejar pedido: {:?}", e);
            }
        });
    }

    println!("Connection closed by client");
    Ok(())
}
