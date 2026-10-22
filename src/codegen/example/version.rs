//! Print the Trader and market-data CTP API versions.

use __CRATE_NAME__::{MdApi, TraderApi};

fn main() {
    println!("Trader API: {}", TraderApi::api_version());
    println!("MD API: {}", MdApi::api_version());
}
