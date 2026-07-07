use std::collections::HashMap;
use std::sync::mpsc;
use std::process::{Command, Stdio};
use std::io::{BufReader, BufRead};
use std::thread;
use std::time::Duration;
use mpris::{LoopStatus, Metadata, PlaybackStatus, Player, PlayerFinder};
use serde::Serialize;
use signal_hook::consts::signal::{SIGUSR1, SIGUSR2};
use signal_hook::iterator::Signals;

enum Signal {
    Event,
    Tick,
    EwwOpen,
    EwwClose,
}

#[derive(Serialize)]
struct PlayerData {
    // data
    title: String,
    art: String,
    artists: String,
    player: String,
    duration: u64,

    // buttons
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
    volume: f64,
    progress: u64,


    // utility
    update_type: i8, // 0 - full, 1 - hard, 2 - soft
    #[serde(skip)]
    last_art_url: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // === SIGNAL EMITTERS ===
    let (tx, rx) = mpsc::channel();

    let tx_event = tx.clone();
    let media_monitor = Command::new("playerctl")
        .arg("--all-players")
        .arg("--follow")
        .arg("--format")
        .arg("{{status}} {{title}} {{artist}}")
        .arg("metadata")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let event_stream = media_monitor.stdout.unwrap();
    let event_reader = BufReader::new(event_stream);
    thread::spawn(move || {
        for _line in event_reader.lines() {
            tx_event.send(Signal::Event).unwrap();
        }
    });

    let tx_tick = tx.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(1));
            tx_tick.send(Signal::Tick).unwrap();
        }
    });

    let tx_eww = tx.clone();
    let mut signals = Signals::new(&[SIGUSR1, SIGUSR2])?;
    thread::spawn(move || {
        for signal in signals.forever() {
            match signal {
                SIGUSR1 => tx_eww.send(Signal::EwwOpen).unwrap(),
                SIGUSR2 => tx_eww.send(Signal::EwwClose).unwrap(),
                _ => {}
            }
        }
    });


    let mut cache: HashMap<String, PlayerData> = HashMap::new();
    let player_finder = PlayerFinder::new()?;

    let mut menu_status = 0; // 0 - closed, 1 - opened
    for signal in rx {
        match signal {
            Signal::EwwOpen => {
                menu_status = 1;
            }
            Signal::EwwClose => {
                menu_status = 0;
                continue;
            }
            Signal::Tick => {
                if menu_status == 0 {
                    continue;
                }
            }
            _ => {}
        };

        let players = player_finder.find_all().unwrap_or_default();
        let do_soft_pull = sync_players(&players, &mut cache);
        
        let do_print = match signal {
            Signal::Event => {
                pull_hard(&players, &mut cache);
                true
            }
            Signal::Tick => {
                if do_soft_pull {
                    pull_soft(&players, &mut cache);
                    true
                }
                else {
                    false
                }
            }
            Signal::EwwOpen => {
                pull_hard(&players, &mut cache);
                true
            }
            _ => false,
        };

        if do_print {
            output_json(&cache);
        }
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
        playing:    match player.get_playback_status().unwrap() {
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


        update_type: 0,
        last_art_url: metadata.art_url().unwrap_or("").to_string(),
    }
}

fn pull_hard(players: &[Player], cache: &mut HashMap<String, PlayerData> ) {
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
            entry.playing =     match player.get_playback_status().unwrap() {
                                PlaybackStatus::Playing => true,
                                _ => false,
                                };

            entry.can_next = player.can_go_next().unwrap_or(false);
            entry.can_previous = player.can_go_previous().unwrap_or(false);
            entry.can_pause = player.can_pause().unwrap_or(false);
            entry.can_shuffle = player.can_shuffle().unwrap_or(false);
            entry.can_loop = player.can_loop().unwrap_or(false);
            entry.can_volume = player.has_volume().unwrap_or(false);
            entry.can_progress = player.can_seek().unwrap_or(false);
                    
            if new_art_url != entry.last_art_url {
                entry.art = download_art(&new_art_url, &id).unwrap_or_default();
                entry.last_art_url = new_art_url;
            }

            entry.update_type = 1;
        }
    }
}

fn pull_soft(players: &[Player], cache: &mut HashMap<String, PlayerData> ) {
    for player in players {
        let id = player.bus_name_trimmed().to_string();

        if !matches!(player.get_playback_status(), Ok(PlaybackStatus::Playing)) {
            continue;
        }

        if let Some(entry) = cache.get_mut(&id) {
            entry.volume = player.get_volume().unwrap_or(0.0);
            entry.progress = player.get_position_in_microseconds().unwrap_or(0);

            entry.update_type = 2;
        }
    }
}

fn download_art(url: &str, player_id: &str) -> Option<String> {
    if let Some(path) = url.strip_prefix("file://") {
        return Some(path.to_string());
    }

    let path = format!("/tmp/eww-art-{}.jpg", player_id);

    let file = reqwest::blocking::get(url).ok()?.bytes().ok()?;
    std::fs::write(&path, file).ok()?;

    Some(path)
}

fn output_json(cache: &HashMap<String, PlayerData>) {
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