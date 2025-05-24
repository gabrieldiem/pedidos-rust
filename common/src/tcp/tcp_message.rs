use actix::Message;

#[derive(Message)]
#[rtype(result = "()")]
pub struct TcpMessage {
    pub data: String,
}
