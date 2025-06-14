use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PortIdPair {
    pub id: u32,
    pub port: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PedidosRustConfig {
    pub ports: Vec<PortIdPair>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CustomerConfig {
    pub ports: Vec<PortIdPair>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Configuration {
    pub pedidos_rust: PedidosRustConfig,
    pub customer: CustomerConfig,
}

impl Configuration {
    pub fn new() -> Result<Configuration, std::io::Error> {
        let json_string = include_str!("./config.json");
        let config: Configuration = serde_json::from_str(json_string)?;
        Ok(config)
    }
}
