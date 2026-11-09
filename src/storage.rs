//! Songs on disk: JSON files in `$XDG_DATA_HOME/backroom` (normally
//! `~/.local/share/backroom`) on Linux, `~/Library/Application Support/Backroom`
//! on macOS. `BACKROOM_DATA` overrides the folder.

use crate::demos;
use crate::pattern::Song;
use std::io;
use std::path::{Path, PathBuf};

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("BACKROOM_DATA") {
        return PathBuf::from(d);
    }
    #[cfg(target_os = "macos")]
    if let Some(h) = std::env::var_os("HOME") {
        return PathBuf::from(h).join("Library/Application Support/Backroom");
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("backroom")
}

/// The song folder for display, with the home folder shortened to `~`.
pub fn display_dir(dir: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        if let Ok(rest) = dir.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    dir.display().to_string()
}

/// A file-name-safe stem: lowercase letters, digits and dashes.
pub fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let s = s.trim_end_matches('-').to_string();
    if s.is_empty() {
        "untitled".into()
    } else {
        s
    }
}

pub fn save(dir: &Path, stem: &str, song: &Song) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{stem}.json"));
    // Write then rename, so a crash never leaves half a file.
    let tmp = dir.join(format!(".{stem}.json.tmp"));
    std::fs::write(&tmp, song.to_json() + "\n")?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

pub fn load(path: &Path) -> Result<Song, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Song::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Song files in the folder, sorted by name: (stem, path).
pub fn list(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| Some((p.file_stem()?.to_str()?.to_string(), p.clone())))
        .filter(|(s, _)| !s.starts_with('.'))
        .collect();
    out.sort();
    out
}

/// Writes the demo songs into the folder if they aren't there yet (never
/// overwrites, so edited demos are kept).
pub fn seed_demos(dir: &Path) -> io::Result<()> {
    for d in demos::all() {
        if !dir.join(format!("{}.json", d.file)).exists() {
            save(dir, d.file, &d.song)?;
        }
    }
    Ok(())
}

/// A stem that doesn't exist yet: `name`, then `name-2`, `name-3`...
pub fn free_stem(dir: &Path, name: &str) -> String {
    let base = slug(name);
    if !dir.join(format!("{base}.json")).exists() {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|s| !dir.join(format!("{s}.json")).exists())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_list_load_and_seed() {
        let dir = std::env::temp_dir().join(format!("backroom-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        seed_demos(&dir).unwrap();
        let files = list(&dir);
        assert_eq!(files.len(), demos::all().len());
        let first = load(&files[0].1).unwrap();
        let mut edited = first.clone();
        edited.name = "My Beat!".into();
        edited.state.tempo = 77.0;
        let stem = free_stem(&dir, &edited.name);
        assert_eq!(stem, "my-beat");
        let p = save(&dir, &stem, &edited).unwrap();
        assert_eq!(load(&p).unwrap(), edited);
        assert_eq!(free_stem(&dir, "My Beat!"), "my-beat-2");
        // Seeding again keeps the user's edits to a demo.
        let demo_path = dir.join(format!("{}.json", demos::all()[0].file));
        save(&dir, demos::all()[0].file, &edited).unwrap();
        seed_demos(&dir).unwrap();
        assert_eq!(load(&demo_path).unwrap().state.tempo, 77.0);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Dusty Boom-Bap"), "dusty-boom-bap");
        assert_eq!(slug("  ??  "), "untitled");
        assert_eq!(slug("A/B..C"), "a-b-c");
    }
}
