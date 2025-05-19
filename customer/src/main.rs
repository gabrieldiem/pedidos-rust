use common::constants::DEFAULT_HOST;
use common::utils::logger::Logger;

use actix::Message;
use actix::{Actor, AsyncContext, Context, Handler};
use actix_async_handler::async_handler;
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

struct CustomerGateway {
    stream_reader: BufReader<TcpStream>,
    stream_writer: TcpStream,
    logger: Logger,
    seq: u32,
}

impl Actor for CustomerGateway {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct DoSomethingMessage(String);

#[async_handler]
impl Handler<DoSomethingMessage> for CustomerGateway {
    type Result = ();

    async fn handle(&mut self, msg: DoSomethingMessage, _ctx: &mut Self::Context) -> Self::Result {
        // tokio::time::sleep(Duration::from_secs(1)).await;
        self.seq += 1;

        // Send to peer phase
        let msg_to_send = msg.0;

        if let Err(e) = self.stream_writer.write_all(msg_to_send.as_bytes()) {
            self.logger
                .error(&format!("Failed to write to stream: {}", e));
            return;
        }

        if let Err(e) = self.stream_writer.write_all(b"\n") {
            self.logger
                .error(&format!("Failed to write to stream: {}", e));
            return;
        }

        // Receive from peer phase
        let mut line: String = String::new();
        if let Err(e) = self.stream_reader.read_line(&mut line) {
            self.logger
                .error(&format!("Failed to read from stream: {}", e));
            return;
        }
        line = line.strip_suffix("\n").unwrap().to_string();
        self.logger.debug(&format!("Received: '{}'", line));

        // Send message to self to loop again
        let next_msg = format!("Please server process my seq={}", self.seq);
        _ctx.address().do_send(DoSomethingMessage(next_msg));
    }
}

#[actix_rt::main]
async fn main() -> io::Result<()> {
    let logger = Logger::new(Some("[CUSTOMER]"));
    logger.info("Starting...");

    let server_sockeaddr_str = format!("{}:{}", DEFAULT_HOST, 7700);
    let stream = TcpStream::connect(server_sockeaddr_str.clone())?;

    logger.info(&format!("Using address {}", stream.local_addr()?));
    logger.info(&format!("Connected to server {}", server_sockeaddr_str));

    let stream_reader = BufReader::new(stream.try_clone()?);
    let stream_writer = stream;

    let customer = CustomerGateway::create(|_ctx| {
        logger.debug("Created CustomerGateway");
        CustomerGateway {
            stream_reader,
            stream_writer,
            logger: Logger::new(Some("[CUSTOMER]")),
            seq: 0,
        }
    });

    let msg = "Hello World!. Seq=0";
    match customer.send(DoSomethingMessage(msg.to_string())).await {
        Ok(_) => {}
        Err(e) => logger.error(&format!("Could not send first message: {e}")),
    }

    Ok(())
}
