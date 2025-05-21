mod cliente_restaurante_de_juguete;

use rand::random;
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    time::sleep,
};

async fn manejar_conexion(socket: TcpStream) -> anyhow::Result<()> {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);
    let mut buffer = String::new();

    let n = reader.read_line(&mut buffer).await?;
    if n == 0 {
        return Ok(()); // conexión cerrada
    }

    let pedido = buffer.trim();
    println!("Pedido recibido: {}", pedido);

    let aceptado = random::<f32>() > 0.1;

    if aceptado {
        writer.write_all(b"OK\n").await?;
        println!("Pedido aceptado: {}", pedido);
        sleep(Duration::from_secs(2)).await; // Quizás debería ser un random
        // Puede ser rechazado luego de haber sido aceptado? Si es así iría acá
        println!("Pedido listo: {}", pedido);
        let respuesta = format!("Pedido '{}' listo\n", pedido);
        writer.write_all(respuesta.as_bytes()).await?;
    } else {
        writer.write_all(b"RECHAZADO\n").await?;
        println!("Pedido rechazado: {}", pedido);
    }

    Ok(())
}

/*
* Recibe constantemente conexiones TCP (van a ser pedidos, TODO implementar un protocolo que verifique esto).
* Por cada pedido se levanta una nueva async task. No se bloquea el hilo principal gracias al funcionamiento de las async tasks,
* y se pueden atender múltiples pedidos al mismo tiempo.

*/
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Restaurante escuchando en 127.0.0.1:8080...");

    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = manejar_conexion(socket).await {
                eprintln!("Error al manejar conexión: {:?}", e);
            }
        });
    }
}
