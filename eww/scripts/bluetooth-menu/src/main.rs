use std::{collections::{BTreeMap, HashMap}, pin::Pin};
use bluer::{self, Adapter, AdapterEvent, Address, Device, DeviceEvent, DeviceProperty::{BatteryPercentage, Connected, Name, Paired}, agent::{self, Agent, ReqError}};
use eww_ipc::EwwClient;
use futures::{stream::{Stream, StreamExt}};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_stream::StreamMap;
use tokio::sync::oneshot;
use serde_json;

#[derive(Clone, Serialize)]
struct DeviceEntry {
    #[serde(skip)]
    device: bluer::Device,
    name: String,
    is_connected: bool,
    is_paired: bool,
    battery: u8,

    status: Status,
    prompt: Option<Prompt>,
}

#[derive(Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Status {
    Idle,
    Connecting,
    Disconnecting,
    Pairing,
    Prompt
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum PromptKind {
    Output,
    Input,
    Confirm
}

#[derive(Clone, Serialize)]
struct Prompt {
    kind: PromptKind,
    value: String
}

struct PromptGuard {
    tx: tokio::sync::mpsc::Sender<Command>,
    addr: Address
}
impl Drop for PromptGuard {
    fn drop(&mut self) {
        let _ = self.tx.try_send(Command::ClosePrompt(self.addr));
    }
}

enum RequestContainer {
    RequestPin(agent::RequestPinCode),
    DisplayPin(agent::DisplayPinCode),
    RequestPasskey(agent::RequestPasskey),
    DisplayPasskey(agent::DisplayPasskey),
    RequestConfirm(agent::RequestConfirmation),
    RequestAuth(agent::RequestAuthorization),
    AuthService(agent::AuthorizeService),
}
impl RequestContainer {
    fn unwrap_request(self) -> (Address, Prompt, Option<oneshot::Receiver<()>>) {
        match self {
            Self::RequestPin(r) => (
                r.device,
                Prompt {
                    kind: PromptKind::Input,
                    value: String::new()
                },
                None
            ),
            Self::DisplayPin(r) => (
                r.device,
                Prompt {
                    kind: PromptKind::Output,
                    value: r.pincode,
                },
                Some(r.cancel)
            ),
            Self::RequestPasskey(r) => (
                r.device,
                Prompt {
                    kind: PromptKind::Input,
                    value: String::new()
                },
                None
            ),
            Self::DisplayPasskey(r) => (
                r.device,
                Prompt {
                    kind: PromptKind::Output,
                    value: format!("{:06}", r.passkey)
                },
                Some(r.cancel)
            ),
            Self::RequestConfirm(r) => (
                r.device,
                Prompt {
                    kind: PromptKind::Confirm,
                    value: format!("{:06}", r.passkey)
                },
                None
            ),
            Self::RequestAuth(r) => (
                r.device,
                Prompt {
                    kind: PromptKind::Confirm,
                    value: String::new()
                },
                None
            ),
            Self::AuthService(r) => (
                r.device,
                Prompt {
                    kind: PromptKind::Confirm,
                    value: r.service.to_string()
                },
                None
            )
        }
    }
}

enum Command {
    // user commands
    ToggleBluetooth,
    ToggleScan,
    Connect(Address),
    Disconnect(Address),
    Pair(Address),
    Forget(Address),

    // pairing agent commangs
    OpenPrompt(Address, Prompt, Option<oneshot::Sender<Option<String>>>),
    ClosePrompt(Address),
    ReplyPrompt(Address, Option<String>),
    Settled(Address),

    CloseMenu
}

type DeviceStream = Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>;
type AdapterStream = Pin<Box<dyn Stream<Item = AdapterEvent> + Send>>;

#[tokio::main]
async fn main() -> bluer::Result<()> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Command>(16);
    let eww_ipc = EwwClient::new()?;

    let mut devices: HashMap<bluer::Address, DeviceEntry> = HashMap::new();
    
    let mut device_events: StreamMap<Address, DeviceStream> = StreamMap::new();
    let mut adapter_events = adapter.events().await?;
    let mut found_events: Option<AdapterStream> = None;

    let mut prompts: HashMap<Address, oneshot::Sender<Option<String>>> = HashMap::new();
    let tx_agent = tx.clone();
    let agent = Agent {
        request_default: true,
        request_pin_code: Some(Box::new({
            let tx = tx_agent.clone();
            move |req| {
                let tx = tx.clone();
                Box::pin(async move {
                    handle_pairing(RequestContainer::RequestPin(req), tx).await
                        .ok_or(ReqError::Rejected)
                })
            }
        })),
        display_pin_code: Some(Box::new({
            let tx = tx_agent.clone();
            move |req| {
                let tx = tx.clone();
                Box::pin(async move {
                    let _ = handle_pairing(RequestContainer::DisplayPin(req), tx).await;
                    Ok(())
                })
            }
        })),
        request_passkey: Some(Box::new({
            let tx = tx_agent.clone();
            move |req| {
                let tx = tx.clone();
                Box::pin(async move {
                    handle_pairing(RequestContainer::RequestPasskey(req), tx).await
                        .and_then(|resp| resp.trim().parse().ok()).ok_or(ReqError::Rejected)
                })
            }
        })),
        display_passkey: Some(Box::new({
            let tx = tx_agent.clone();
            move |req| {
                let tx = tx.clone();
                Box::pin(async move {
                    let _ = handle_pairing(RequestContainer::DisplayPasskey(req), tx).await;
                    Ok(())
                })
            }
        })),
        request_confirmation: Some(Box::new({
            let tx = tx_agent.clone();
            move |req| {
                let tx = tx.clone();
                Box::pin(async move {
                    handle_pairing(RequestContainer::RequestConfirm(req), tx).await
                        .ok_or(ReqError::Rejected).map(|_| ())
                })
            }
        })),
        request_authorization: Some(Box::new({
            let tx = tx_agent.clone();
            move |req| {
                let tx = tx.clone();
                Box::pin(async move {
                    handle_pairing(RequestContainer::RequestAuth(req), tx).await
                        .ok_or(ReqError::Rejected).map(|_| ())
                })
            }
        })),
        authorize_service: Some(Box::new({
            let tx = tx_agent.clone();
            move |req| {
                let tx = tx.clone();
                Box::pin(async move {
                    handle_pairing(RequestContainer::AuthService(req), tx).await
                        .ok_or(ReqError::Rejected).map(|_| ())
                })
            }
        })),
        ..Default::default()
    };
    let _agent_handle = session.register_agent(agent).await?;
    
    let tx_fifo = tx.clone();
    tokio::spawn( async move {
        let path = "/tmp/eww-bluetooth-menu.fifo".to_string();
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
    

    for addr in adapter.device_addresses().await? {
        add_device(&adapter, addr, &mut devices, &mut device_events, false).await?;
    }
    output_devices(&devices, &eww_ipc);
    let _ = eww_ipc.update(&[
        ("bt_scanning", "false".to_string()),
        ("bt_on", adapter.is_powered().await?.to_string())
    ]);

    let tx_loop = tx.clone();
    loop {
        tokio::select! {
            Some(event) = adapter_events.next() => {
                handle_adapter_event(event, &adapter, &mut devices, &mut device_events).await?;
                output_devices(&devices, &eww_ipc);
            }
            Some((addr, event)) = device_events.next() => {
                handle_device_event(event, addr, &mut devices)?;
                output_devices(&devices, &eww_ipc);
            }
            Some(event) = async {
                found_events.as_mut().unwrap().next().await
            }, if found_events.is_some() => {
                handle_adapter_event(event, &adapter, &mut devices, &mut device_events).await?;
                output_devices(&devices, &eww_ipc);
            }
            Some(cmd) = rx.recv() => {
                handle_command(cmd, &adapter, &mut devices, &mut found_events, &mut device_events, &mut prompts, &tx_loop, &eww_ipc).await?;
                output_devices(&devices, &eww_ipc);
            }
        }
    }
}

async fn handle_pairing(request: RequestContainer, tx: tokio::sync::mpsc::Sender<Command>) -> Option<String> {
    let (addr, prompt, cancel) = request.unwrap_request();
    let _guard = PromptGuard { tx: tx.clone(), addr };
    
    if let Some(cancel) = cancel {
        tx.send(Command::OpenPrompt(addr, prompt, None)).await.ok()?;
        let _ = cancel.await;
        return None;
    }

    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(Command::OpenPrompt(addr, prompt, Some(reply_tx))).await.ok()?;
    reply_rx.await.ok().flatten()    
}

fn parse_command(line: &str) -> Option<Command> {
    let mut split = line.split_whitespace();
    let cmd = split.next()?;
    
    match cmd {
        "toggle_scan" => return Some(Command::ToggleScan),
        "toggle_bluetooth" => return Some(Command::ToggleBluetooth),
        "close" => return Some(Command::CloseMenu),
        _ => {}
    }

    let addr = split.next()?.parse::<Address>().ok();
    Some(match cmd {
        "connect" => Command::Connect(addr?),
        "disconnect" => Command::Disconnect(addr?),
        "pair" => Command::Pair(addr?),
        "forget" => Command::Forget(addr?),

        "answer" => {
            let answer = split.collect::<Vec<&str>>().join(" ");
            Command::ReplyPrompt(addr?, match answer.as_str() {
                "" | "no" => None,
                "yes" => Some(String::new()),
                _ => Some(answer),
            })
        },
        _ => return None,
    })
}

async fn handle_command(cmd: Command, adapter: &Adapter, devices: &mut HashMap<bluer::Address, DeviceEntry>, found: &mut Option<AdapterStream>, events: &mut StreamMap<Address, DeviceStream>, prompts: &mut HashMap<Address, oneshot::Sender<Option<String>>>, tx: &tokio::sync::mpsc::Sender<Command>, eww: &EwwClient) -> bluer::Result<()> {
    match cmd {
        Command::ToggleBluetooth => {
            if adapter.is_powered().await? {
                let _ = eww.update(&[
                    ("bt_on", "false".to_string()),
                    ("bt_scanning", "false".to_string())
                ]);
                *found = None;
                adapter.set_powered(false).await?;
            }
            else {
                let _ = eww.update(&[("bt_on", "true".to_string())]);
                adapter.set_powered(true).await?;
            }
        }
        Command::ToggleScan => {
            if found.is_some() {
                *found = None;
                let _ = eww.update(&[("bt_scanning", "false".to_string())]);
            }
            else {
                let _ = eww.update(&[("bt_scanning", "true".to_string())]);
                *found = Some(adapter.discover_devices().await?.boxed());
            }
        }
        Command::Connect(addr) => {
            if let Some(entry) = devices.get_mut(&addr) {
                adapter.set_powered(true).await?;
                let _ = eww.update(&[("bt_on", "true".to_string())]);

                entry.status = Status::Connecting;
                let dev = entry.device.clone();
                let tx = tx.clone();
                tokio::spawn(async move{
                    let _ = dev.connect().await;
                    let _ = tx.send(Command::Settled(addr)).await;
                });
            }
        }
        Command::Disconnect(addr) => {
            if let Some(entry) = devices.get_mut(&addr) {
                entry.status = Status::Disconnecting;
                let dev = entry.device.clone();
                let tx = tx.clone();
                tokio::spawn(async move{
                    let _ = dev.disconnect().await;
                    let _ = tx.send(Command::Settled(addr)).await;
                });
            }
        }
        Command::Forget(addr) => {
            let adapter = adapter.clone();
            tokio::spawn(async move {
                let _ = adapter.remove_device(addr).await;
            });
        }
        Command::Pair(addr) => {
            if let Some(entry) = devices.get_mut(&addr) {
                entry.status = Status::Pairing;
                let dev = entry.device.clone();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let _ = pair_device(dev).await;
                    let _ = tx.send(Command::Settled(addr)).await;
                });
            }
            
        }
        Command::OpenPrompt(addr, prompt, repl ) => {
            add_device(adapter, addr, devices, events, true).await?;
            if let Some(repl) = repl {
                prompts.insert(addr, repl);
            }
            if let Some(entry) = devices.get_mut(&addr) {
                entry.status = Status::Prompt;
                entry.prompt = Some(prompt);
            }
        }
        Command::ReplyPrompt(addr, answer) => {
            if let Some(repl) = prompts.remove(&addr) {
                let _ = repl.send(answer);
            }
            if let Some(entry) = devices.get_mut(&addr) {
                entry.status = Status::Pairing;
                entry.prompt = None;
            }
        }
        Command::ClosePrompt(addr) => {
            prompts.remove(&addr);
            if let Some(entry) = devices.get_mut(&addr) {
                if entry.status == Status::Prompt {
                    entry.status = Status::Pairing;
                }
                entry.prompt = None;
            }
        }
        Command::Settled(addr) => {
            if let Some(entry) = devices.get_mut(&addr) {
                if entry.status != Status::Prompt {
                    entry.status = Status::Idle;
                }
            }
        }
        Command::CloseMenu => {
            if found.is_some() {
                *found = None;
                let _ = eww.update(&[("bt_scanning", "false".to_string())]);
            }
            
        }
    }
    Ok(())
}

async fn pair_device(device: Device) -> bluer::Result<()> {
    device.pair().await?;
    device.set_trusted(true).await?;
    device.connect().await?;
    
    Ok(())
}

fn handle_device_event(event: DeviceEvent, addr: Address, devices: &mut HashMap<bluer::Address, DeviceEntry>) -> bluer::Result<()> {
    let Some(device) = devices.get_mut(&addr) else { return Ok(()); };
    match event {
        bluer::DeviceEvent::PropertyChanged(Name(val)) => device.name = val,
        bluer::DeviceEvent::PropertyChanged(Paired(val)) => {
            device.is_paired = val;
            if device.status != Status::Prompt {
                device.status = Status::Idle;
            }
        },
        bluer::DeviceEvent::PropertyChanged(Connected(val)) => {
            device.is_connected = val;
            if device.status != Status::Prompt {
                device.status = Status::Idle;
            }
        },
        bluer::DeviceEvent::PropertyChanged(BatteryPercentage(val)) => {
            if device.is_paired { device.battery = val; }
        },
        _ => {}
    }

    Ok(())
}



async fn handle_adapter_event(event: AdapterEvent, adapter: &Adapter, devices: &mut HashMap<bluer::Address, DeviceEntry>, events: &mut StreamMap<Address, DeviceStream>) -> bluer::Result<()> {
    match event {
        AdapterEvent::DeviceAdded(addr) => {
            add_device(adapter, addr, devices, events, false).await?;
        }
        AdapterEvent::DeviceRemoved(addr) => {
            devices.remove(&addr);
            events.remove(&addr);
        }
        _ => {}
    }

    Ok(())
}

async fn add_device(adapter: &Adapter, addr: Address, devices: &mut HashMap<bluer::Address, DeviceEntry>, events: &mut StreamMap<Address, DeviceStream>, force: bool) -> bluer::Result<()> {
    if devices.contains_key(&addr) { return Ok(()); }
    
    let device = adapter.device(addr)?;
    let name = device.name().await?.unwrap_or_default();
    let is_paired = device.is_paired().await?;

    if !force && name.is_empty() && !is_paired { return Ok(()); }

    let is_connected = device.is_connected().await?;
    let battery = device.battery_percentage().await?.unwrap_or_default();

    events.insert(addr, device.events().await?.boxed());
    devices.insert(addr, DeviceEntry { 
        device: device, 
        name, 
        is_connected,
        is_paired,
        battery,

        status: Status::Idle,
        prompt: None
    });

    Ok(())
}

fn output_devices(devices: &HashMap<bluer::Address, DeviceEntry>, eww: &EwwClient) {
    let map: BTreeMap<&Address, &DeviceEntry> = devices.iter().collect();

    let mut addresses_connected: Vec<&Address> = Vec::new();
    let mut addresses_paired: Vec<&Address> = Vec::new();
    let mut addresses_found: Vec<&Address> = Vec::new();

    for (addr, dev) in devices {
        let is_connected = dev.is_connected;
        let is_paired = dev.is_paired;

        if is_connected {
            addresses_connected.push(addr);
            continue;
        }
        if is_paired {
            addresses_paired.push(addr);
            continue;
        }
        addresses_found.push(addr);
    }
    let devices_json = serde_json::to_string(&map).unwrap_or_default();
    println!("{}", devices_json);

    addresses_connected.sort_unstable();
    addresses_paired.sort_unstable();
    addresses_found.sort_unstable();

    let _ = eww.update(&[
        ("bt_addresses_connected", serde_json::to_string(&addresses_connected).unwrap_or_default()),
        ("bt_addresses_paired", serde_json::to_string(&addresses_paired).unwrap_or_default()),
        ("bt_addresses_found", serde_json::to_string(&addresses_found).unwrap_or_default())
    ]);
}