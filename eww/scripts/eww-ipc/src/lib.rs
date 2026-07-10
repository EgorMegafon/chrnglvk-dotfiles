use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use eww_shared_util::VarName;
use serde::Serialize;
use simplexpr::dynval::DynVal;

#[derive(Serialize)]
enum Action {
    #[allow(dead_code)]
    Ping, 
    Update { mappings: Vec<(VarName, DynVal)> }
}

pub struct EwwClient {
    socket_path: PathBuf,
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

            Ok(Self {socket_path},)
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
}

#[cfg(test)]
mod tests {

use super::*;

    #[test]
    fn test () {
        let client = EwwClient::new().unwrap();
        client.update(&[("media_art_rotation", "5".to_string())]).unwrap();
    }
}