use std::{collections::{BTreeMap, HashMap}};
use eww_ipc::EwwClient;
use futures::StreamExt;
use nmrs::{NetworkEvent, NetworkManager};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::Command::{Connect, ToggleWifi};

#[derive(Serialize, Clone)]
struct NetworkEntry {
    is_active: bool,
    is_known: bool,
    strength: u8,
    entering_password: bool
}

enum Command {
    ToggleWifi,
    Connect(String),
    Disconnect(String),
    EnterPassword(String, String),
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
    nm.set_wireless_enabled(true).await?;
    update(&mut networks, &nm).await?;
    output_soft(&networks);

    let mut events = nm.network_events().await?;

    loop {
        tokio::select! {
            Some(event) = events.next() => {
                handle_event(event?, &mut networks, &nm).await?;
            }
            Some(cmd) = rx.recv() => {
                handle_command(cmd, &mut networks, &nm, &eww_ipc).await?;
            }
        }
    }
}

async fn handle_command(cmd: Command, cache: &mut HashMap<String, NetworkEntry>, nm: &NetworkManager, eww: &EwwClient) -> nmrs::Result<()> {
    match cmd {
        ToggleWifi => {
            let current_state = nm.wifi_state().await?;
            let current_bool_state = current_state.enabled && current_state.hardware_enabled && current_state.present;
            nm.set_wireless_enabled(!current_bool_state).await?;
            let _ = eww.update(&[("wifi_on", (!current_bool_state).to_string())]);
        }
        Connect(ssid) => {
            if !nm.has_saved_connection(&ssid).await? && nm.{
                cache.get_mut(&ssid);
            }
            nm.try_connect(&ssid, None, creds);
        }

    }
    Ok(())
}

fn parse_command(line: &str) -> Option<Command> {
    let mut split = line.split_whitespace();
    let cmd = split.next()?;
    
    match cmd {
        "toggle_wifi" => return Some(Command::ToggleWifi),
        "close" => return Some(Command::CloseMenu),
        _ => {}
    }

    let ssid = split.next()?.parse::<String>().ok();
    Some(match cmd {
        "connect" => Command::Connect(ssid?),
        "disconnect" => Command::Disconnect(ssid?),

        "password" => {
            let answer = split.collect::<Vec<&str>>().join(" ");
            Command::EnterPassword(ssid?, answer)
        },
        _ => return None,
    })
}

async fn handle_event(event: NetworkEvent, cache: &mut HashMap<String, NetworkEntry>, nm: &NetworkManager) -> nmrs::Result<()> {

    Ok(())
}

async fn update(cache: &mut HashMap<String, NetworkEntry>, nm: &NetworkManager) -> nmrs::Result<()> {
    let networks = nm.list_networks(None).await?;
    let mut current_ssids: Vec<String> = Vec::new();

    for network in networks {
        if let Some(saved) = cache.get_mut(&network.ssid) {
            saved.is_active = network.is_active.clone();
            saved.is_known = network.known.clone();
            saved.strength = network.strength.unwrap_or_default().clone();
        }
        else {
            let new = NetworkEntry {
                is_active: network.is_active.clone(),
                is_known: network.known.clone(),
                strength: network.strength.unwrap_or_default().clone(),
                entering_password: false
            };
            cache.insert(network.ssid.clone(), new);
        }
        current_ssids.push(network.ssid);
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

    for (ssid, network) in networks {
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
    ]);
}