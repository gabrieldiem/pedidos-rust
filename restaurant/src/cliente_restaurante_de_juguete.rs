use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    time::{sleep, Duration},
};


/*
   Agrego este ejemplo de juguete para probar que el restaurante funciona. El ejemplo es este:

    🕐 t = 0s

        Llega Pedido A. Se acepta, comienza a "cocinarse" con sleep(2s) dentro de una task.

    🕐 t = 1s

        Llega Pedido B. Se acepta también. Se lanza una nueva task y también empieza a cocinarse (sleep(2s)).

    🕑 t = 2s

        Pedido A termina su sleep, se envía la respuesta "Pedido A listo".

    🕒 t = 3s

        Pedido B termina su sleep, se envía la respuesta "Pedido B listo".
*/
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Spawn para Pedido A (en t=0s)
    let pedido_a = tokio::spawn(async {
        hacer_pedido("Pedido A").await;
    });

    // Esperamos 1 segundo para lanzar Pedido B
    sleep(Duration::from_secs(1)).await;

    // Spawn para Pedido B (en t=1s)
    let pedido_b = tokio::spawn(async {
        hacer_pedido("Pedido B").await;
    });

    // Esperamos que ambos terminen
    let _ = tokio::join!(pedido_a, pedido_b);

    Ok(())
}

async fn hacer_pedido(pedido: &str) {
    match TcpStream::connect("127.0.0.1:8080").await {
        Ok(mut stream) => {
            println!("[{}] Conectado al restaurante", pedido);

            let pedido_str = format!("{}\n", pedido);
            if let Err(e) = stream.write_all(pedido_str.as_bytes()).await {
                eprintln!("[{}] Error al enviar pedido: {:?}", pedido, e);
                return;
            }
            println!("[{}] Pedido enviado", pedido);

            let mut reader = BufReader::new(stream);
            let mut buffer = String::new();

            if let Err(e) = reader.read_line(&mut buffer).await {
                eprintln!("[{}] Error al leer respuesta: {:?}", pedido, e);
                return;
            }

            let respuesta = buffer.trim();
            println!("[{}] Respuesta inicial: {}", pedido, respuesta);

            if respuesta == "OK" {
                buffer.clear();
                if let Err(e) = reader.read_line(&mut buffer).await {
                    eprintln!("[{}] Error al leer respuesta final: {:?}", pedido, e);
                    return;
                }
                println!("[{}] Respuesta final: {}", pedido, buffer.trim());
            } else {
                println!("[{}] Pedido fue rechazado", pedido);
            }
        }
        Err(e) => eprintln!("[{}] Error al conectar: {:?}", pedido, e),
    }
}
