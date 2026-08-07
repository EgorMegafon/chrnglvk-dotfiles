use std::collections::{BTreeMap, HashMap};
use std::time::SystemTime;
use eww_ipc::EwwClient;
use futures::StreamExt;
use nmrs::{ConnectionError, DeviceState, NetworkEvent, NetworkManager, SettingsPatch, WifiSecurity};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};

const UPDATE_INTERVAL: u64 = 1;

#[derive(Serialize, Clone)]
struct NetworkEntry {
    is_active: bool,
    is_known: bool,
    strength: u8,
    autoconnect: bool,
    entering_password: bool
}

enum Command {
    ToggleWifi,
    Connect(String),
    ToggleAutoconnect(String),
    Forget(String),
    Disconnect,
    EnterPassword(String, String),
    CancelPassword(String),
    CloseMenu
}

#[tokio::main]
async fn main() -> nmrs::Result<()> {
    let nm = NetworkManager::new().await?;
    let eww_ipc = EwwClient::new().unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Command>(16);

    let tx_fifo = tx.clone();
    tokio::spawn( async move {
        let path = "/tmp/eww-network-menu.fifo".to_string();
        std::process::Command::new("mkfifo")
            .arg(&path)
            .status().ok();
        let file = match tokio::fs::OpenOptions::new().read(true).write(true).open(&path).await {
            Ok(f) => f,
            _ => return
        };
        let mut lines = BufReader::new(file).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(cmd) = parse_command(&line) {
                tx_fifo.send(cmd).await.unwrap();
            }
        }
    });

    let mut networks: HashMap<String, NetworkEntry> = HashMap::new();

    let state = nm.wifi_state().await?;
    let wifi_enabled = state.enabled && state.hardware_enabled && state.present;
    let _ = eww_ipc.update(&[("wifi_on", wifi_enabled.to_string())]);
    update(&mut networks, &nm).await?;
    output_hard(&networks, &eww_ipc);
    output_soft(&networks);

    let mut last_update = SystemTime::now();
    let mut events = nm.network_events().await?;

    loop {
        tokio::select! {
            Some(event) = events.next() => {
                match handle_event(event?, &mut networks, &nm, &mut last_update).await {
                    Ok(_) => {
                        output_hard(&networks, &eww_ipc);
                        output_soft(&networks);
                    }
                    Err(_) => {}
                }
            }
            Some(cmd) = rx.recv() => {
                handle_command(cmd, &mut networks, &nm, &eww_ipc).await?;
                output_hard(&networks, &eww_ipc);
                output_soft(&networks);
            }
        }
    }
}

async fn set_autoconnect(nm: &NetworkManager, ssid: &str, enabled: bool) -> nmrs::Result<()> {
    let Some(uuid) = nm.get_saved_connection_uuid(ssid).await? else { return Ok(()) };
    let mut patch = SettingsPatch::default();
    patch.autoconnect = Some(enabled);
    nm.update_saved_connection(&uuid, patch).await
}

async fn handle_command(cmd: Command, cache: &mut HashMap<String, NetworkEntry>, nm: &NetworkManager, eww: &EwwClient) -> nmrs::Result<()> {
    match cmd {
        Command::ToggleWifi => {
            let current_state = nm.wifi_state().await?;
            let current_bool_state = current_state.enabled && current_state.hardware_enabled && current_state.present;
            nm.set_wireless_enabled(!current_bool_state).await?;
            let _ = eww.update(&[("wifi_on", (!current_bool_state).to_string())]);
        }
        Command::Connect(ssid) => {
            let Some(network) = nm.list_networks(None).await?
                .into_iter()
                .find(|net| net.ssid == ssid)
            else { return Ok(()); };
            let Some(net) = cache.get_mut(&ssid) else { return Ok(()) };

            let creds: WifiSecurity = match (network.secured, network.known) {
                (false, _) => WifiSecurity::Open,
                (true, true) => WifiSecurity::WpaPsk { psk: String::new() },
                (true, false) => {
                    net.entering_password = true;
                    return Ok(());
                }
            };

            match nm.try_connect(&ssid, None, creds).await {
                Ok(_) => { net.entering_password = false; }
                Err(_) => { net.entering_password = true; }
            }
        }
        Command::EnterPassword(ssid, psk) => {
            if psk.is_empty() { return Ok(()) }
            let _ = nm.forget(&ssid).await;
            let creds: WifiSecurity = WifiSecurity::WpaPsk { psk };
            let Some(net) = cache.get_mut(&ssid) else { return Ok(()) };
            match nm.try_connect(&ssid, None, creds).await {
                Ok(_) => { net.entering_password = false; }
                Err(_) => { net.entering_password = false; }
            }
        }
        Command::CancelPassword(ssid) => {
            let Some(net) = cache.get_mut(&ssid) else { return Ok(()) };
            net.entering_password = false;
        }
        Command::Disconnect => {
            nm.disconnect(None).await?;
        }
        Command::ToggleAutoconnect(ssid) => {
            let Some(net) = cache.get_mut(&ssid) else { return Ok(()) };
            set_autoconnect(nm, &ssid, !net.autoconnect).await?;
            net.autoconnect = !net.autoconnect;
        }
        Command::Forget(ssid) => {
            let _ = nm.forget(&ssid).await;
            if let Some(net) = cache.get_mut(&ssid) {
                net.is_known = false;
                net.entering_password = false;
            }
        }
        Command::CloseMenu => {
            for net in cache.values_mut() {
                net.entering_password = false;
            }
        }
    }
    Ok(())
}

fn parse_command(line: &str) -> Option<Command> {
    let mut parts = line.split('\t');
    let cmd = parts.next()?.trim();

    match cmd {
        "toggle_wifi" => return Some(Command::ToggleWifi),
        "close" => return Some(Command::CloseMenu),
        "disconnect" => return Some(Command::Disconnect),
        _ => {}
    }

    let ssid = parts.next()?.to_string();
    Some(match cmd {
        "connect" => Command::Connect(ssid),
        "cancel_password" => Command::CancelPassword(ssid),
        "toggle_autoconnect" => Command::ToggleAutoconnect(ssid),
        "forget"=> Command::Forget(ssid),
        "password" => Command::EnterPassword(ssid, parts.next()?.to_string()),
        _ => return None,
    })
}

async fn handle_event(_: NetworkEvent, cache: &mut HashMap<String, NetworkEntry>, nm: &NetworkManager, last_update: &mut SystemTime) -> nmrs::Result<()> {
    let interval = SystemTime::now().duration_since(*last_update).unwrap_or_default();
    if interval.as_secs() < UPDATE_INTERVAL { return Err(ConnectionError::Timeout); }

    update(cache, nm).await?;
    *last_update = SystemTime::now();
    Ok(())
}

async fn update(cache: &mut HashMap<String, NetworkEntry>, nm: &NetworkManager) -> nmrs::Result<()> {
    let networks = nm.list_networks(None).await?;
    let ethernet = nm.list_wired_device_details().await?;
    let mut current_ssids: Vec<String> = Vec::new();

    for network in networks {
        if let Some(saved) = cache.get_mut(&network.ssid) {
            saved.is_active = network.is_active;
            saved.is_known = network.known;
            saved.strength = network.strength.unwrap_or_default();
        }
        else {
            let new = NetworkEntry {
                is_active: network.is_active,
                is_known: network.known,
                strength: network.strength.unwrap_or_default(),
                autoconnect: false,
                entering_password: false
            };
            cache.insert(network.ssid.clone(), new);
        }
        current_ssids.push(network.ssid);
    }

    if ethernet.first().is_some() {
        let dev = ethernet.first().unwrap();
        let is_active = dev.state == DeviceState::Activated;

        if let Some(saved) = cache.get_mut("Ethernet") {
            saved.is_active = is_active;
        }
        else {
            let new = NetworkEntry {
                is_active: is_active,
                is_known: true,
                strength: 100,
                autoconnect: true,
                entering_password: false
            };
            cache.insert("Ethernet".to_string(), new);
        }
        current_ssids.push("Ethernet".to_string());
    }


    let removed_ssids: Vec<String> = cache
        .keys()
        .filter(|ssid| !current_ssids.contains(*ssid))
        .cloned()
        .collect();
    for ssid in removed_ssids {
        let _ = cache.remove(&ssid);
    }

    Ok(())
}

fn output_soft(networks: &HashMap<String, NetworkEntry>) {
    let map: BTreeMap<&String, &NetworkEntry> = networks.iter().collect();
    let ssids_json = serde_json::to_string(&map).unwrap_or_default();
    println!("{}", ssids_json);
}

fn output_hard(networks: &HashMap<String, NetworkEntry>, eww: &EwwClient) {
    let mut active: Vec<&String> = Vec::new();
    let mut known: Vec<&String> = Vec::new();
    let mut found: Vec<&String> = Vec::new();
    
    let mut ethernet: Vec<String> = Vec::new();

    for (ssid, network) in networks {
        if network.entering_password == true { return; }
        
        if ssid == "Ethernet" {
            ethernet.push(ssid.clone());
            continue;
        }

        if network.is_active { 
            active.push(ssid);
            continue;
        }
        else if network.is_known {
            known.push(ssid);
            continue;
        }
        found.push(ssid);
    }

    let _ = eww.update(&[
        ("networks_active",  serde_json::to_string(&active).unwrap_or_default()),
        ("networks_known",  serde_json::to_string(&known).unwrap_or_default()),
        ("networks_found",  serde_json::to_string(&found).unwrap_or_default()),
        ("ethernet", serde_json::to_string(&ethernet).unwrap_or_default())
    ]);
}