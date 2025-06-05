//! Constants used throughout the project

pub const DEFAULT_PR_HOST: &str = "127.0.0.1";
pub const DEFAULT_PR_PORT: u32 = 7700;

pub const MIN_ORDER_DURATION: u64 = 1; // Minimum order duration in seconds
pub const MAX_ORDER_DURATION: u64 = 10; // Maximum order duration in seconds
pub const PAYMENT_REJECTED_PROBABILITY: f32 = 0.2;
pub const ORDER_REJECTED_PROBABILITY: f32 = 0.1;
