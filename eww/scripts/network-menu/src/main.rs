use std::collections::HashMap;
use serde::Serialize;
use serde_json;
use zbus;


#[derive(Serialize)]
struct NetworkEntry {
    ssid: String,
    strength: u8,
    is_connected: bool,
    is_authorized: bool,
    is_ethernet: bool
}
#[derive(Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Status {
    Connected,
    ConnectedNoInternet,
    Available,
    Found,
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.AccessPoint",
    default_service = "org.freedesktop.NetworkManager",
)]
trait AccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> zbus::Result<Vec<u8>>;
    #[zbus(property)]
    fn strength(&self) -> zbus::Result<u8>;
}




fn main() -> Result<(), Box<dyn std::error::Error>> {
    let networks: HashMap<String, NetworkEntry> = HashMap::new();
    let ssids: HashMap<String, Status> = HashMap::new();



    Ok(())
}

fn add_network(networks: &mut HashMap<String, NetworkEntry>, ifaces: &mut HashMap<String, Status>) {
    
}