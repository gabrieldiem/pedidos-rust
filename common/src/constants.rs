//! Constants used throughout the project

pub const DEFAULT_PR_HOST: &str = "127.0.0.1";
pub const DEFAULT_PR_PORT: u32 = 7501;
pub const DEFAULT_PAYMENT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PAYMENT_PORT: u32 = 9000;

pub const MIN_ORDER_DURATION: u64 = 1; // In seconds
pub const MAX_ORDER_DURATION: u64 = 10; // In seconds
pub const PAYMENT_REJECTED_PROBABILITY: f32 = 0.2;
pub const ORDER_REJECTED_PROBABILITY: f32 = 0.1;
pub const PAYMENT_DURATION: u64 = 1; // In seconds

pub const NO_RESTAURANTS: &str = "No hay restaurantes disponibles";
pub const MAX_DISTANCE_RESTAURANTS: u32 = 10;
pub const MIN_ORDER_PRICE: f64 = 5.0;
pub const MAX_ORDER_PRICE: f64 = 100.0;
pub const N_RIDERS_TO_NOTIFY: usize = 2;
