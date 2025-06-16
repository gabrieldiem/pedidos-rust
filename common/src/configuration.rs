use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SinglePedidosRustInfo {
    pub id: u32,
    pub port: u32,
    pub ports_for_peers: Vec<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SingleCustomerInfo {
    pub id: u32,
    pub port: u32,
    pub x: u16,
    pub y: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SingleRiderInfo {
    pub id: u32,
    pub port: u32,
    pub x: u16,
    pub y: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PedidosRustConfig {
    pub infos: Vec<SinglePedidosRustInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RiderConfig {
    pub infos: Vec<SingleCustomerInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CustomerConfig {
    pub infos: Vec<SingleCustomerInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Configuration {
    pub pedidos_rust: PedidosRustConfig,
    pub customer: CustomerConfig,
    pub rider: RiderConfig,
}

impl Configuration {
    pub fn new() -> Result<Configuration, std::io::Error> {
        let json_string = include_str!("./config.json");
        let config: Configuration = serde_json::from_str(json_string)?;
        Ok(config)
    }
}
