use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use eww_shared_util::VarName;
use serde::{Deserialize, Serialize};
use simplexpr::dynval::DynVal;

#[derive(Serialize)]
enum Action {
    #[allow(dead_code)]
    Ping, 
    #[allow(dead_code)]
    Update { mappings: Vec<(VarName, DynVal)> },
    #[allow(dead_code)]
    Poll,
    #[allow(dead_code)]
    OpenInspector,
    #[allow(dead_code)]
    OpenWindow,
    #[allow(dead_code)]
    OpenMany,
    #[allow(dead_code)]
    CloseSelected,
    #[allow(dead_code)]
    Reload,
    #[allow(dead_code)]
    KillServer,
    #[allow(dead_code)]
    CloseAll,
    #[allow(dead_code)]
    ShowState,
    #[allow(dead_code)]
    GetVar { name: String },
}

#[derive(Clone)]
pub struct EwwClient {
    socket_path: PathBuf,
}

#[derive(Deserialize)]
enum Responce {
    Succsess(String),
    Failure(())
}

impl EwwClient {
    pub fn new() -> std::io::Result<Self> {
        let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());

        let socket_path = std::fs::read_dir(&dir)?
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("eww-server_"))
            })
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "eww socket not found"
                )
            })?;

            Ok(Self {socket_path})
   }

    pub fn update(&self, mappings: &[(&str, String)]) -> std::io::Result<()> {
        let action = Action::Update { 
            mappings: mappings
                .iter()
                .map(|(name, value)| {
                    (VarName::from(name.to_string()), DynVal::from(value.clone()))
                })
                .collect(),
        };

        let bytes = bincode::serialize(&action)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut stream = UnixStream::connect(&self.socket_path)?;
        stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
        stream.write_all(&bytes)?;
        stream.flush()?;
        
        Ok(())
    }

    pub fn get(&self, var_name: &str) -> std::io::Result<Option<String>> {
        let action = Action::GetVar { name: var_name.to_string() };

        let bytes = bincode::serialize(&action)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut stream = UnixStream::connect(&self.socket_path)?;
        stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
        stream.write_all(&bytes)?;
        stream.flush()?;

        stream.set_read_timeout(Some(Duration::from_millis(100)))?;
        let mut buf = Vec::new();
        match stream.read_to_end(&mut buf) {
            Ok(_) => {}
            Err(e) => return Err(e)
        };

        if buf.is_empty() {
            return Ok(None);
        }

        let response: Responce = bincode::deserialize(&buf)
            .map_err(|e|  std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        Ok(match response {
            Responce::Succsess(r) => Some(r),
            _ => None,
        })

    }
}

#[cfg(test)]
mod tests {

use super::*;

    #[test]
    fn test () {
        let client = EwwClient::new().unwrap();
        println!("{}", client.get("media_art_rotation").unwrap().unwrap());
        client.update(&[("media_art_rotation", "10".to_string())]).unwrap();
        println!("{}", client.get("media_art_rotation").unwrap().unwrap());
    }
}