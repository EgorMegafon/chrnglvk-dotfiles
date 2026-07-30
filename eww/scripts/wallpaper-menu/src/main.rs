use std::collections::{HashMap, HashSet};
use std::{println};
use std::env::home_dir;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use image::imageops::FilterType;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

const CACHEFILE: &str = ".cache/eww/wallpaper-cache.json";
const WPDIR: &str = "Pictures/Wallpapers";
const THUMBDIR: &str = ".cache/eww/wallpaper-thumbs";
const THUMB_W: u32 = 384;
const THUMB_H: u32 = 216;


#[derive(Serialize, Deserialize, Clone)]
struct WallpaperEntry {
    path: PathBuf,
    thumb: PathBuf,
    colors: Vec<String>
}


fn main() -> notify::Result<()> {
    let (tx, rx) = mpsc::channel();
    let full_dir = Path::new(&home_dir().unwrap()).join(WPDIR);

    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    let watcher_dir = full_dir.clone();
    watcher.watch(Path::new(&watcher_dir), RecursiveMode::NonRecursive)?;

    let mut cache: HashMap<PathBuf, WallpaperEntry> = load_cache();

    delete_wallpapers(&full_dir, &mut cache)?;
    create_wallpapers(&full_dir, &mut cache)?;
    output(&cache);

    for event in rx {
        match event {
            Ok(e) => {
                match e.kind {
                    EventKind::Create(_) => create_wallpapers(&full_dir, &mut cache)?,
                    EventKind::Remove(_) => delete_wallpapers(&full_dir, &mut cache)?,
                    EventKind::Modify(_) => {
                        delete_wallpapers(&full_dir, &mut cache)?;
                        create_wallpapers(&full_dir, &mut cache)?;
                    }
                    _ => {}
                }
                output(&cache);
            }
            _ => {}
        }
        
    }
    Ok(())
}

fn create_wallpapers(path: &PathBuf, cache: &mut HashMap<PathBuf, WallpaperEntry>) -> io::Result<()> {
    let mut new_paths: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if cache.get(&path).is_some_and(|entry| entry.thumb.exists()) { continue; }

        let mut file = File::open(&path)?;
        let mut buffer = [0; 40];
        let _ = file.read(&mut buffer);
        match infer::get(&buffer) {
            Some(kind) if kind.matcher_type() == infer::MatcherType::Image => {}
            _ => continue,
        }
        new_paths.push(path);
    }

    let new_wallpapers: Vec<WallpaperEntry> = std::thread::scope(|scope| {
        let handles: Vec<_> = new_paths
            .into_iter()
            .map(|path| {
                scope.spawn(move || {
                    let colors = calculate_colors(path.to_str().unwrap_or_default());
                    let thumb = generate_thumbnail(&path);
                    WallpaperEntry { path, thumb, colors}
                })
            })
            .collect();

        handles.into_iter().filter_map(|handle| handle.join().ok()).collect()
    });

    for wallpaper in new_wallpapers {
        cache.insert(wallpaper.path.clone(), wallpaper);
    }

    output_cache(&cache);
    Ok(())
}

fn delete_wallpapers(path: &PathBuf, cache: &mut HashMap<PathBuf, WallpaperEntry>) -> io::Result<()> {
    let mut current_paths: HashSet<PathBuf> = HashSet::new();
    for entry in fs::read_dir(path)? {
        current_paths.insert(entry?.path());
    }

    let removed_paths: Vec<PathBuf> = cache
        .keys()
        .filter(|path| !current_paths.contains(*path))
        .cloned()
        .collect();

    for path in removed_paths {
        if let Some(entry) = cache.remove(&path) {
            let _ = fs::remove_file(&entry.thumb);
        }
    }

    output_cache(&cache);
    Ok(())
}

fn generate_thumbnail(src: &Path) -> PathBuf {
    let cache = home_dir().unwrap().join(THUMBDIR);
    let _ = fs::create_dir_all(&cache);

    let mut name_bytes = [0u8; 8];
    rand::fill(&mut name_bytes);
    let name = hex::encode(name_bytes);
    let dst = cache.join(format!("{}.png", name));

    let img = image::open(src).unwrap();
    let _ = img.resize_to_fill(THUMB_W, THUMB_H, FilterType::Triangle)
        .save(&dst);

    dst
}

fn calculate_colors(path: &str) -> Vec<String> {
    let children: Vec<_> = (0..4)
        .filter_map(|i| {
            Command::new("matugen")
                .args([
                    "image", path,
                    "--dry-run",
                    "-q", "-j", "hex",
                    "--source-color-index", &i.to_string(),
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .ok()
        })
        .collect();

    let results: Vec<Option<String>> = children
        .into_iter()
        .map(|child| {
            let out = child.wait_with_output().ok()?;
            if !out.status.success() {
                return None;
            }
            let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
            Some(json["colors"]["source_color"]["default"]["color"].as_str()?.to_string())
        })
        .collect();

    let output: Vec<String> = results.into_iter().map_while(|c| c).collect();

    output
}

fn load_cache() -> HashMap<PathBuf, WallpaperEntry> {
    let path = home_dir().unwrap().join(CACHEFILE);
    fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn output_cache(cache: &HashMap<PathBuf, WallpaperEntry>) {
    let save_path = home_dir().unwrap().join(CACHEFILE);
    if let Some(parent) = save_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string(cache).unwrap_or_default();
    fs::write(&save_path, &json).ok();
}

fn output(cache: &HashMap<PathBuf, WallpaperEntry>) {
    let entries: Vec<WallpaperEntry> = cache
        .values().cloned().collect();
    let json = serde_json::to_string(&entries).unwrap_or_default();
    println!("{}", json);
}