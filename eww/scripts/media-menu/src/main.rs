use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use dbus::message::MatchRule;
use eww_ipc::EwwClient;
use mpris::{LoopStatus, Metadata, PlaybackStatus, Player, PlayerFinder};
use serde::Serialize;
use dbus::blocking::Connection;
use std::process::Command;
use std::io::{BufRead, BufReader, Read};
use std::fs::OpenOptions;



enum Signal {
    Event,
    Tick,
    EwwOpen,
    EwwClose,
    ForceSoftRefrech,
}

enum WidgetStatus {
        MenuOpened,
        MenuClosed,
}

#[derive(Serialize)]
struct PlayerData {
    // data
    title: String,
    art: String,
    artists: String,
    player: String,
    duration: u64,

    // controls
    shuffle: bool,
    loop_status: String,
    playing: bool,
    
    // buttons availability
    can_next: bool,
    can_previous: bool,
    can_pause: bool,
    can_shuffle: bool,
    can_loop: bool,
    can_volume: bool,
    can_progress: bool,


    // fast
    progress: u64,
    volume: f64,


    // utility
    #[serde(skip)]
    last_art_url: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // === SIGNAL EMITTERS ===
    let (tx, rx) = mpsc::channel();

    let tx_bus_event = tx.clone();
    let conn = Connection::new_session()?;
    let mut conn_filter = MatchRule::new_signal(
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
        );
    conn_filter.path = Some("/org/mpris/MediaPlayer2".into());

    conn.add_match(conn_filter, move |args: (String, dbus::arg::PropMap, Vec<String>), _, _| {
        let (_iface, changed, _inval) = args;
        let only_volume = !changed.is_empty() && changed.keys().all(|k| k =="Volume");

        if !only_volume {
            let _ = tx_bus_event.send(Signal::Event);
        }
        true
    })?;

    thread::spawn(move || {
        loop {
            conn.process(std::time::Duration::from_millis(1000)).ok();
        }
    });

    let tx_tick = tx.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(1));
            tx_tick.send(Signal::Tick).unwrap();
        }
    });

    let tx_user_event = tx.clone();
    thread::spawn(move || {
        let path = "/tmp/eww-media-menu.fifo";
        Command::new("mkfifo").arg(path).status().ok();

        loop {
            let file = match OpenOptions::new().read(true).open(path) {
                Ok(f) => f,
                _ => { 
                    thread::sleep(Duration::from_millis(208));
                    continue;
                }
            };
            for line in BufReader::new(file).lines().flatten() {
                let signal = match line.trim() {
                    "open" => Signal::EwwOpen,
                    "close" => Signal::EwwClose,
                    "soft" => Signal::ForceSoftRefrech,
                    "hard" => Signal::Event,
                    _ => continue,
                };
                let _ = tx_user_event.send(signal);
            }
        }
    });




    // === MAIN SHTUKA ===
    let mut cache: HashMap<String, PlayerData> = HashMap::new();
    let player_finder = PlayerFinder::new()?;

    let eww_ipc = EwwClient::new().unwrap();

    let mut menu_status = WidgetStatus::MenuClosed;
    for signal in rx {
        match signal {
            Signal::EwwOpen => {
                menu_status = WidgetStatus::MenuOpened;
            }
            Signal::EwwClose => {
                menu_status = WidgetStatus::MenuClosed;
                continue;
            }
            Signal::Tick => {
                if matches!(menu_status, WidgetStatus::MenuClosed) {
                    continue;
                }
            }
            _ => {}
        };

        let players = player_finder.find_all().unwrap_or_default();
        let do_soft_pull = sync_players(&players, &mut cache);
        
        match signal {
            Signal::Event => {
                pull_hard(&players, &mut cache);
                output_hard(&cache);
                pull_soft(&players, &mut cache, true);
                output_soft(&cache, &eww_ipc);
            }
            Signal::Tick => {
                if do_soft_pull {
                    pull_soft(&players, &mut cache, false);
                    output_soft(&cache,&eww_ipc);
                }
            }
            Signal::EwwOpen => {
                pull_hard(&players, &mut cache);
                output_hard(&cache);
                pull_soft(&players, &mut cache, true);
                output_soft(&cache, &eww_ipc);
            }
            Signal::ForceSoftRefrech => {
                pull_soft(&players, &mut cache, true);
                output_soft(&cache, &eww_ipc);
            }
            _ => {}
        };
    }

    Ok(())
}

fn sync_players(players: &[Player], cache: &mut HashMap<String, PlayerData>) -> bool {
    let mut something_playing = false;

    let current_ids: Vec<String> = players.iter().map(|player| player.bus_name_trimmed().to_string()).collect();
    cache.retain(|id, _| current_ids.contains(id));
    for player in players {
        let id = player.bus_name_trimmed().to_string();
        if !cache.contains_key(&id) {
            cache.insert(id.clone(), pull_full(player));
        }
        match player.get_playback_status() {
            Ok(PlaybackStatus::Playing) => { something_playing = true; }
            _ => {}
        }
    }

    return something_playing
}

fn pull_full(player: &Player) -> PlayerData {
    let metadata = match player.get_metadata() {
        Ok(meta) => meta,
        _ => Metadata::default(),
    };

    return PlayerData {
        // hard
        title: metadata.title().unwrap_or("Unknown").to_string(),
        art: metadata.art_url()
            .and_then(|url| download_art(url, &player.bus_name_trimmed()))
            .unwrap_or_default(),
        artists: metadata.artists().unwrap_or_default().join(", ").to_string(),
        player: player.bus_name_trimmed().to_lowercase(),
        duration: metadata.length_in_microseconds().unwrap_or(0),

        shuffle: player.get_shuffle().unwrap_or(false),
        loop_status:    match player.get_loop_status() {
                            Ok(LoopStatus::Track) => "track".to_string(),
                            Ok(LoopStatus::Playlist) => "playlist".to_string(),
                            _ => "none".to_string(),
                        },
        playing:    match player.get_playback_status().unwrap_or(PlaybackStatus::Paused) {
                            PlaybackStatus::Playing => true,
                            _ => false,
                        },
        
        can_next: player.can_go_next().unwrap_or(false),
        can_previous: player.can_go_previous().unwrap_or(false),
        can_pause: player.can_pause().unwrap_or(false),
        can_shuffle: player.can_shuffle().unwrap_or(false),
        can_loop: player.can_loop().unwrap_or(false),
        can_volume: player.has_volume().unwrap_or(false),
        can_progress: player.can_seek().unwrap_or(false),


        // soft
        volume: player.get_volume().unwrap_or(0.0),
        progress: player.get_position_in_microseconds().unwrap_or(0),

        last_art_url: metadata.art_url().unwrap_or("").to_string(),
    }
}

fn pull_hard(players: &[Player], cache: &mut HashMap<String, PlayerData>) {
    for player in players {
        let id = player.bus_name_trimmed().to_string();
        let metadata = player.get_metadata().unwrap_or_default();
        let new_art_url = metadata.art_url().unwrap_or("").to_string();

        if let Some(entry) = cache.get_mut(&id) {
            let new_title = metadata.title().unwrap_or("Unknown").to_string();

            if entry.title != new_title {
                entry.progress = 0;
            }

            entry.title = new_title;
            entry.artists = metadata.artists().unwrap_or_default().join(", ").to_string();
            entry.player = player.bus_name_trimmed().to_lowercase();
            entry.duration = metadata.length_in_microseconds().unwrap_or(0);

            entry.shuffle = player.get_shuffle().unwrap_or(false);
            entry.loop_status =    match player.get_loop_status() {
                                    Ok(LoopStatus::Track) => "track".to_string(),
                                    Ok(LoopStatus::Playlist) => "playlist".to_string(),
                                    _ => "none".to_string(),
                                    };
            match player.get_playback_status().unwrap_or(PlaybackStatus::Paused) {
                PlaybackStatus::Playing => {
                    entry.playing = true;
                }
                _ => {
                    entry.playing = false;
                }
            }
            

            entry.can_next = player.can_go_next().unwrap_or(false);
            entry.can_previous = player.can_go_previous().unwrap_or(false);
            entry.can_pause = player.can_pause().unwrap_or(false);
            entry.can_shuffle = player.can_shuffle().unwrap_or(false);
            entry.can_loop = player.can_loop().unwrap_or(false);
            entry.can_volume = player.has_volume().unwrap_or(false);
            entry.can_progress = player.can_seek().unwrap_or(false);
                    
            if !new_art_url.is_empty() && new_art_url != entry.last_art_url {
                entry.art = download_art(&new_art_url, &id).unwrap_or_default();
                entry.last_art_url = new_art_url.to_string();
            }
        }
    }
}

fn pull_soft(players: &[Player], cache: &mut HashMap<String, PlayerData>, force: bool ) {
    for player in players {
        let id = player.bus_name_trimmed().to_string();

        if !matches!(player.get_playback_status(), Ok(PlaybackStatus::Playing)) && !force {
            continue;
        }

        if let Some(entry) = cache.get_mut(&id) {
            entry.progress = player.get_position_in_microseconds().unwrap_or(0);
            entry.volume = player.get_volume().unwrap_or(entry.volume);
        }
    }
}

fn download_art(url: &str, player_id: &str) -> Option<String> {
    if let Some(path) = url.strip_prefix("file://") {
        return Some(path.to_string());
    }

    let path = format!("/tmp/eww-art-{}.jpg", player_id);

    let file = ureq::get(url).call();
    let mut bytes = Vec::new();
    match file {
        Ok(f) => {
            f.into_body().into_reader().read_to_end(&mut bytes).ok()?;
        }
        _ => { return None; }
    }

    std::fs::write(&path, bytes).ok()?;

    Some(path)
}



fn output_hard(cache: &HashMap<String, PlayerData>) {
    fn priority(player_name: &str) -> u8 {
        return match player_name {
            "spotify" => 0,
            "firefox" => 1,
            _ => 2,
        }
    }

    let mut cache_entries: Vec<&PlayerData> = cache.values().collect();
    cache_entries.sort_by_key(|entry| priority(&entry.player));
    println!("{}", serde_json::to_string(&cache_entries).unwrap());
}

#[derive(Serialize)]
struct SoftOutput {
    progress: u64, 
    volume: f64,
}

fn output_soft(cache: &HashMap<String, PlayerData>, eww_ipc: &EwwClient) {
    let output_values: HashMap<&str, SoftOutput> = cache.values()
        .map(
            |entry| (entry.player.as_str(), 
            SoftOutput { progress: entry.progress, volume: entry.volume } )
        ).collect();
    let json = serde_json::to_string(&output_values).unwrap();

    eww_ipc.update(&[("soft_all_media_updates", json)]).unwrap();
}