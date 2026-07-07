use niri_ipc::{Request, Response};
use niri_ipc::socket::Socket;
use niri_ipc::Event;

use std::io::{Write, stdout};


fn main() -> std::io::Result<()> {
    let mut socket = Socket::connect()?;

    let mut layout_names:  Vec<String> = Vec::new(); 
    let reply = socket.send(Request::EventStream)?;

    if matches!(reply, Ok(Response::Handled)) {
        let mut read_event = socket.read_events();
        while let Ok(event) = read_event() {
            match event {
                Event::KeyboardLayoutsChanged { keyboard_layouts } => {
                    layout_names = keyboard_layouts.names;
                    
                }
                Event::KeyboardLayoutSwitched { idx } => {
                    let short = match layout_names[idx as usize].as_str() {
                        "English (US)" => "en",
                        "Russian"      => "ru",
                        other => &other[..2].to_lowercase(),
                    };
                    println!("{}", short);
                    stdout().flush()?;
                }
                _ => {}
            }
        }
    }

    Ok(())
}