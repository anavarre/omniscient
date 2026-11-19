//! Where the database lives, and who decided.
//!
//! The location can't be stored in the database it points to, so it lives in
//! a one-line config file. Precedence, highest first:
//!
//! 1. `--db <path>` (this launch only)
//! 2. `OMNISCIENT_DB` (this launch only)
//! 3. `db_path` in `$XDG_CONFIG_HOME/omniscient/config.toml` (what the
//!    in-app "Move database" writes)
//! 4. `$XDG_DATA_HOME/omniscient/omniscient.sqlite3`

use std::io::Write;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Flag,
    Env,
    Config,
    Default,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Flag => "flag",
            Source::Env => "env",
            Source::Config => "config",
            Source::Default => "default",
        }
    }

    /// Only the config file and the default survive a restart, so only they
    /// can be changed from inside the app.
    pub fn movable(self) -> bool {
        matches!(self, Source::Config | Source::Default)
    }
}

#[derive(Clone, Debug)]
pub struct Location {
    pub path: PathBuf,
    pub source: Source,
    pub config_file: PathBuf,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(fallback))
}

pub fn default_db() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("omniscient").join("omniscient.sqlite3")
}

pub fn config_file() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("omniscient").join("config.toml")
}

pub fn resolve(flag: Option<PathBuf>) -> Location {
    resolve_with(flag, std::env::var_os("OMNISCIENT_DB").map(PathBuf::from), config_file())
}

pub fn resolve_with(flag: Option<PathBuf>, env: Option<PathBuf>, config_file: PathBuf) -> Location {
    let (path, source) = if let Some(p) = flag {
        (p, Source::Flag)
    } else if let Some(p) = env.filter(|p| !p.as_os_str().is_empty()) {
        (p, Source::Env)
    } else if let Some(p) = read_config(&config_file) {
        (p, Source::Config)
    } else {
        (default_db(), Source::Default)
    };
    Location { path, source, config_file }
}

/// The `db_path` key; anything unreadable or relative counts as unset so a
/// damaged config can never send the app somewhere surprising.
pub fn read_config(file: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(file).ok()?;
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        if key.trim() != "db_path" {
            return None;
        }
        let value = value.trim().strip_prefix('"')?.strip_suffix('"')?;
        let path = PathBuf::from(value.replace("\\\"", "\"").replace("\\\\", "\\"));
        path.is_absolute().then_some(path)
    })
}

/// Atomic: a crash leaves the old config or the new one, never half of one.
pub fn write_config(file: &Path, db: &Path) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("Cannot save {}: {e}", file.display());
    let dir = file.parent().ok_or("Bad config path")?;
    std::fs::create_dir_all(dir).map_err(fail)?;
    let tmp = dir.join(format!(".config.toml.tmp-{}", std::process::id()));
    let value = db.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        writeln!(f, "# Written by Omniscient (Settings → Database location).")?;
        writeln!(f, "db_path = \"{value}\"")?;
        f.sync_all()?;
        std::fs::rename(&tmp, file)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(fail)
}

/// What the user typed: `~` expanded, a directory gets the default file name.
pub fn parse_destination(input: &str) -> Result<PathBuf, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter a path.".into());
    }
    let expanded = match input.strip_prefix('~') {
        Some("") => home(),
        Some(rest) if rest.starts_with('/') => home().join(&rest[1..]),
        _ => PathBuf::from(input),
    };
    if !expanded.is_absolute() {
        return Err("Use an absolute path (starting with / or ~).".into());
    }
    if input.ends_with('/') || expanded.is_dir() {
        return Ok(expanded.join("omniscient.sqlite3"));
    }
    Ok(expanded)
}

// ------------------------------------------------------------ one window per database

static HELD: Mutex<Option<(UnixListener, PathBuf)>> = Mutex::new(None);

fn socket_for(db: &Path) -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let tag: u64 = db.to_string_lossy().bytes().fold(1469598103934665603, |h, b| (h ^ b as u64).wrapping_mul(1099511628211));
    Some(PathBuf::from(runtime).join(format!("omniscient-{tag:016x}.sock")))
}

/// Takes the "one window per database" claim for the app's lifetime (and
/// releases any earlier one). False if another instance already owns `db`.
pub fn hold_instance(db: &Path) -> bool {
    let Some(sock) = socket_for(db) else { return true };
    if std::os::unix::net::UnixStream::connect(&sock).is_ok() {
        return false;
    }
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).ok();
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, old)) = held.take() {
        if let Some(old_sock) = socket_for(&old) {
            let _ = std::fs::remove_file(old_sock);
        }
    }
    *held = listener.map(|l| (l, db.to_path_buf()));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("omniscient-loc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn precedence_is_flag_env_config_default() {
        let dir = tmp("prec");
        let cfg = dir.join("config.toml");
        write_config(&cfg, Path::new("/data/cfg.sqlite3")).unwrap();
        let at = |flag: Option<&str>, env: Option<&str>| resolve_with(flag.map(PathBuf::from), env.map(PathBuf::from), cfg.clone());
        assert_eq!((at(Some("/f"), Some("/e")).path, at(Some("/f"), Some("/e")).source), (PathBuf::from("/f"), Source::Flag));
        assert_eq!((at(None, Some("/e")).path, at(None, Some("/e")).source), (PathBuf::from("/e"), Source::Env));
        assert_eq!(at(None, Some("")).path, PathBuf::from("/data/cfg.sqlite3"));
        assert_eq!(at(None, None).source, Source::Config);
        std::fs::remove_file(&cfg).unwrap();
        assert_eq!(at(None, None).source, Source::Default);
        assert!(!Source::Flag.movable() && !Source::Env.movable() && Source::Config.movable() && Source::Default.movable());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn config_round_trips_awkward_paths_and_ignores_damage() {
        let dir = tmp("cfg");
        let cfg = dir.join("sub/config.toml");
        let odd = Path::new("/mnt/my \"data\"/back\\slash/o.sqlite3");
        write_config(&cfg, odd).unwrap();
        assert_eq!(read_config(&cfg).as_deref(), Some(odd));
        assert!(std::fs::read_dir(cfg.parent().unwrap()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains("tmp")));
        for bad in ["db_path = relative/x", "db_path = \"relative/x\"", "db_path = \"/unterminated", "garbage", ""] {
            std::fs::write(&cfg, bad).unwrap();
            assert_eq!(read_config(&cfg), None, "{bad:?}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn destinations_are_parsed_strictly() {
        assert!(parse_destination("  ").is_err());
        assert!(parse_destination("relative/x.db").is_err());
        assert_eq!(parse_destination("/a/b.db").unwrap(), PathBuf::from("/a/b.db"));
        assert_eq!(parse_destination("/a/b/").unwrap(), PathBuf::from("/a/b/omniscient.sqlite3"));
        let dir = tmp("dest");
        assert_eq!(parse_destination(dir.to_str().unwrap()).unwrap(), dir.join("omniscient.sqlite3"));
        assert_eq!(parse_destination("~/x.db").unwrap(), home().join("x.db"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
