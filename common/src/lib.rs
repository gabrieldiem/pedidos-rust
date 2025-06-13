pub fn a_common_function(message: &str) {
    println!("[COMMON] Message is: {message}");
}

pub mod configuration;
pub mod constants;
pub mod protocol;
pub mod tcp;
pub mod utils;
