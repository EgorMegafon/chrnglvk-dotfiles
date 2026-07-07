use std::process::{Command, Stdio};
use std::io::{BufReader, BufRead};
use std::sync::atomic::AtomicI16;
use std::sync::Arc;
use std::sync::atomic::{Ordering};
use std::thread;
use signal_hook::consts::signal::SIGUSR1;
use signal_hook::iterator::Signals;
use std::sync::mpsc;
use mpris::{PlaybackStatus, Player, PlayerFinder};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Spotify,
    Others,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    // === PLAYERCTL EVENT THREAD ===
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
            tx_event.send(()).unwrap();
        }
    });


    // === EWW SIGNAL THREAD ===
    let tx_signal = tx.clone();
    let mode_flag = Arc::new(AtomicI16::new(0));
    let flag_for_thread = Arc::clone(&mode_flag);
    let mut signals = Signals::new(&[SIGUSR1])?;
    thread::spawn(move || {
        for _ in signals.forever() {
            let current_mode = flag_for_thread.load(Ordering::Relaxed);
            let mut added_mode = current_mode + 1;
            if added_mode > 1 { added_mode = 0};
            flag_for_thread.store(added_mode, Ordering::Relaxed);
            tx_signal.send(()).unwrap();
        }
    });




    // === MAIN THREAD ===
    let player_finder = PlayerFinder::new()?;
    let mut last_media = "".to_string();
    let mut last_status = PlaybackStatus::Stopped;
    for _ in rx {
        let mode = match mode_flag.load(Ordering::Relaxed) {
            0 => Mode::Spotify,
            1 => Mode::Others,
            _ => Mode::Others
        };

        match find_player(&player_finder, &mode) {
            Some(player) => {
                let title = match player.get_metadata() {
                    Ok(data) => data.title().unwrap_or("nonenone").to_string(),
                    _ => "nonenone".to_string(),
                };
                let status = match player.get_playback_status() {
                    Ok(stat) => stat,
                    _ => PlaybackStatus::Stopped,
                };
                if title != last_media || status != last_status {
                    last_media = title;
                    last_status = status;

                    update_metadata(&player);
                }
            }
            None => {}
        }

        
    }
    Ok(())
}

fn find_player(finder: &PlayerFinder, mode: &Mode) -> Option<Player> {
    let players = finder.find_all().ok()?;
    let mode = match mode {
        Mode::Spotify => 0,
        Mode::Others => 1,
    };

    let mut spotify_present = false;
    let mut fallback_non_spotify: Option<Player> = None;

    for player in &players {
        if player.identity() == "Spotify" {
            spotify_present = true;
        }
    }

    for player in players {
        let is_spotify = player.identity() == "Spotify";
        if mode == 0 && spotify_present {
            if is_spotify {
                return Some(player)
            }
            else {
                continue;
            }
        }
        if is_spotify {
            continue;
        }
        
        match player.get_playback_status() {
            Ok(status) => {
                match status {
                    PlaybackStatus::Playing => {
                        return Some(player)
                    }
                    PlaybackStatus::Paused => {
                        if fallback_non_spotify.is_none() {
                            fallback_non_spotify = Some(player);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    
    return fallback_non_spotify
}

fn update_metadata(player: &Player) {
    let metdata = player.get_metadata();
    match metdata {
        Ok(data) => {
            let title = data.title().unwrap_or("unknown");
            let artists = data.artists().unwrap_or_default().join(", ");
            let status = match player.get_playback_status() {
                Ok(state) => state,
                _ => PlaybackStatus::Paused,
            };

            let output = serde_json::json!({
                "title": title,
                "artists": artists,
                "player": &player.bus_name_trimmed().to_lowercase(),
                "status": match status {
                    PlaybackStatus::Playing => "playing",
                    _ => "paused",
                }
            });

            println!("{}", output);
        }
        _ => {}
    }
}