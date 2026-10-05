use std::os::unix::fs::FileTypeExt;
use std::sync::mpsc;
use std::{format, thread};
use std::process::Command;
use std::fs::OpenOptions;
use std::time::Duration;
use std::io::{BufReader, BufRead};
use eww_ipc::{self, EwwClient};
use std::sync::Arc;
use std::sync::{Mutex, Condvar};

enum Signal {
    OpenMediaOutputs,
    CloseMediaoutputs,
    OpenMediaInputs,
    CloseMediaInputs,
}

struct SpinArt {
    media_menu_opened: bool,
    active_media_playing: bool,
}

fn main() {
    let (tx, rx) = mpsc::channel();
    let eww = EwwClient::new().unwrap();

    
    let media_state = Arc::new((Mutex::new(SpinArt { media_menu_opened: false, active_media_playing: false }), Condvar::new()));
    let media_state_fifo = media_state.clone();

    let tx_ui_event = tx.clone();
    thread::spawn(move || {
        let path = "/tmp/eww-animation-controller.fifo";
        ensure_fifo(path);

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
                    "open_media_outputs" => Signal::OpenMediaOutputs,
                    "close_media_outputs" => Signal::CloseMediaoutputs,
                    "open_media_inputs" => Signal::OpenMediaInputs,
                    "close_media_inputs" => Signal::CloseMediaInputs,
                    
                    "media_menu_opened" => {
                        set_state(&media_state_fifo, |s| s.media_menu_opened = true);
                        continue
                    }
                    "media_menu_closed" => {
                        set_state(&media_state_fifo, |s| s.media_menu_opened = false);
                        continue
                    }
                    "active_media_playing" => {
                        set_state(&media_state_fifo, |s| s.active_media_playing = true);
                        continue
                    }
                    "active_media_paused" => {
                        set_state(&media_state_fifo, |s| s.active_media_playing = false);
                        continue
                    }
                    _ => continue,
                };
                let _ = tx_ui_event.send(signal);
            }
        }
    });


    
    
    let rotate_art = media_state.clone();
    let eww_for_thread = eww.clone();
    thread::spawn(move || {
        let (lock, cvar) = &*rotate_art;
        let mut media_art_rotation = 0.0;
        loop {
            {
                let mut guard = lock.lock().unwrap();
                while !(guard.media_menu_opened && guard.active_media_playing) {
                    guard = cvar.wait(guard).unwrap();
                }
            }

            media_art_rotation = (media_art_rotation + 0.1) % 100.0;
            eww_for_thread
                .update(&[("media_art_rotation", format!("{media_art_rotation:.2}"))])
                .unwrap_or_default();
            thread::sleep(Duration::from_millis(20));
        }
    });

    for signal in rx {
        match signal {
            Signal::OpenMediaOutputs => {
                tween("media_outputs_reveal_button_rotation", 0.0, 50.0, 100, eww.clone());
            }
            Signal::CloseMediaoutputs => {
                tween("media_outputs_reveal_button_rotation", 50.0, 0.0, 100, eww.clone());
            }
            Signal::OpenMediaInputs => {
                tween("media_inputs_reveal_button_rotation", 0.0, 50.0, 100, eww.clone());
            }
            Signal::CloseMediaInputs => {
                tween("media_inputs_reveal_button_rotation", 50.0, 0.0, 100, eww.clone());
            }
        }
    }

}

fn tween(var: &'static str, from: f64, to: f64, time_ms: u64, eww: EwwClient) {
    const FRAMERATE:u64 = 60;
    

    let frametime = Duration::from_millis(1000 / FRAMERATE);
    let frames_ammount = (time_ms * FRAMERATE / 1000).max(1);
    let step = (to - from) / frames_ammount as f64;

    thread::spawn(move || {
        for i in 1..frames_ammount {
            let value = from + step * i as f64;
            let _ = eww.update(&[(var, format!("{value:.2}"))]);
            thread::sleep(frametime);
        }
        let _ = eww.update(&[(var, to.to_string())]);
    });
}

fn set_state(state: &Arc<(Mutex<SpinArt>, Condvar)>, f: impl FnOnce(&mut SpinArt)) {
    let (lock, cvar) = &**state;
    f(&mut lock.lock().unwrap());
    cvar.notify_all();
}

fn ensure_fifo(path: &str) {
    match std::fs::metadata(path) {
        Ok(m) if m.file_type().is_fifo() => return,
        Ok(_) => { let _ = std::fs::remove_file(path); }
        Err(_) => {}
    }
    let _ = Command::new("mkfifo").arg(path).status();
}