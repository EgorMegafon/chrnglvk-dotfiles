use std::sync::mpsc;
use std::thread;
use std::process::Command;
use std::fs::OpenOptions;
use std::time::Duration;
use std::io::{BufReader, BufRead};
use eww_ipc::{self, EwwClient};

enum Signal {
    OpenMediaOutputs,
    CloseMediaoutputs,
    OpenMediaInputs,
    CloseMediaInputs,
}

fn main() {
    let (tx, rx) = mpsc::channel();
    let eww = EwwClient::new().unwrap();
    
    let tx_ui_event = tx.clone();
    thread::spawn(move || {
        let path = "/tmp/eww-animation-controller.fifo";
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
                    "open_media_outputs" => Signal::OpenMediaOutputs,
                    "close_media_outputs" => Signal::CloseMediaoutputs,
                    "open_media_inputs" => Signal::OpenMediaInputs,
                    "close_media_inputs" => Signal::CloseMediaInputs,
                    _ => continue,
                };
                let _ = tx_ui_event.send(signal);
            }
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
