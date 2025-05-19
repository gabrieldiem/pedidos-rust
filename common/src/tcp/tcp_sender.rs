use crate::tcp::tcp_message::TcpMessage;
use actix::{Actor, Context, Handler};
use actix_async_handler::async_handler;
use tokio::io::{AsyncWriteExt, WriteHalf};
use tokio::net::TcpStream;

pub struct TcpSender {
    pub write_stream: Option<WriteHalf<TcpStream>>,
}

impl Actor for TcpSender {
    type Context = Context<Self>;
}

#[async_handler]
impl Handler<TcpMessage> for TcpSender {
    type Result = ();

    async fn handle(&mut self, msg: TcpMessage, _ctx: &mut Self::Context) -> Self::Result {
        let mut write_stream = self.write_stream.take().expect(
            "No debería poder llegar otro mensaje antes de que vuelva por usar AtomicResponse",
        );

        let ret_write = async move {
            write_stream
                .write_all(msg.0.as_bytes())
                .await
                .expect("should have sent");
            write_stream
        }
        .await;

        self.write_stream = Some(ret_write);
    }
}
