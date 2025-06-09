use actix::Message;
use serde::Serialize;

#[derive(Message)]
#[rtype(result = "()")]
pub struct TcpMessage {
    pub data: String,
}

/// Creates a `TcpMessage` from a serialized JSON response.
///
/// # Arguments
/// * `response` - A reference to a serializable object that will be converted to JSON.
///
/// # Returns
/// - `Ok(TcpMessage)`: if serialization is successful, containing the JSON string with a newline appended.
/// - `Err(String)`: if serialization fails, containing an error message.
///
/// The message contains the serialized JSON data followed by a newline character, which is often used to delimit messages in network protocols.
impl TcpMessage {
    pub fn from_serialized_json<T: Serialize>(response: &T) -> Result<Self, String> {
        let msg_to_send = serde_json::to_string(response)
            .map_err(|e| format!("Failed to serialize message: {}", e))?;
        Ok(TcpMessage {
            data: msg_to_send + "\n",
        })
    }
}
