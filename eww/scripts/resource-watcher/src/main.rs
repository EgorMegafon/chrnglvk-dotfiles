use std::thread;
use std::time::Duration;
use nvml_wrapper::{Nvml};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use eww_ipc::EwwClient;

fn main() {
    let eww = EwwClient::new().unwrap();

    let mut system = System::new_with_specifics(
        RefreshKind::nothing()
        .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
        .with_memory(MemoryRefreshKind::nothing().with_ram())   
    );

    let mut nvml: Option<Nvml> = None;

    loop {
        let open = eww.get("reveal_resource_usage").unwrap().unwrap_or("".to_string()) == "true";
        thread::sleep(Duration::from_secs(1));
        if open { 
            if nvml.is_none() {
                nvml = Nvml::init().ok();
            }
            let vram = nvml.as_ref()
                .and_then(|nv| nv.device_by_index(0).ok())
                .and_then(|gpu| gpu.memory_info().ok());

            system.refresh_cpu_specifics(CpuRefreshKind::nothing().with_cpu_usage());
            system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
            
            let cpu_display = (system.global_cpu_usage() as f64 * 10.0).round() / 10.0;
            let ram_display = (system.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0 * 10.0).round() / 10.0;
            let vram_display = match vram.as_ref() {
                Some(vram) => (vram.used as f64 / 1024.0 / 1024.0 / 1024.0 * 10.0).round() / 10.0,
                None => 0.0
            };

            let output = serde_json::json!({
                "cpu": cpu_display,
                "ram": ram_display,
                "vram": vram_display,
            });
            println!("{}", output)
        }

        else {
            nvml = None;
        }
    };

    
}
