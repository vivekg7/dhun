//! This computer's own settings and cursors, the phone's `Prefs` and the
//! Mac's. A JSON file in the app's folder rather than GSettings: GSettings
//! needs a schema compiled and installed beside the app, for a handful of
//! values only this app reads.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Values {
    pub server: String,
    pub user: String,
    pub admin: bool,
    pub server_version: String,
    pub library_version: i64,
    pub library_format: i64,
    pub sync_version: i64,
    pub device_id: i64,
    /// The queue playing when the app last stopped.
    pub active_queue: String,
    /// When this device last reported its playback, and the hand-off last dismissed (plan 017).
    pub state_at: i64,
    pub handoff_dismissed: String,
    /// The listen still open, saved every few seconds, so a crash loses none (plan 008).
    pub open_listen: String,
    /// What each queue was played from, logged with its listens: `album:<id>` and the like.
    pub sources: std::collections::HashMap<String, String>,
    /// This computer's limits, in GB, 0 for none, and downloading on a
    /// metered network (plan 012): None until set, for the phone's defaults.
    pub cache_limit_gb: Option<u64>,
    pub download_limit_gb: Option<u64>,
    pub download_on_metered: Option<bool>,
    /// This computer's everyday speed and pitch (plan 016); 0 for unset, read as 1.
    pub speed: f64,
    pub semitones: i64,
    /// The sleep timer's "Let the song finish", remembered (plan 034).
    pub sleep_finish_song: bool,
    /// system, light, dark, black or time (day and night by the clock); "" for the default.
    pub theme: String,
    /// One of the eight accents, Dull Orange first.
    pub palette: usize,
}

impl Values {
    pub fn cache_limit(&self) -> u64 {
        self.cache_limit_gb.unwrap_or(3) * crate::downloads::GB
    }
    pub fn download_limit(&self) -> u64 {
        self.download_limit_gb.unwrap_or(10) * crate::downloads::GB
    }
    pub fn everyday(&self) -> crate::tempo::Tempo {
        let speed = if self.speed > 0.0 { self.speed } else { 1.0 };
        crate::tempo::Tempo { speed, semitones: self.semitones }
    }
}

pub struct Prefs {
    file: PathBuf,
    token_file: PathBuf,
    values: Mutex<Values>,
    token: Mutex<Option<String>>,
}

impl Prefs {
    pub fn open(dir: &Path) -> Prefs {
        let file = dir.join("prefs.json");
        let values = fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Prefs { file, token_file: dir.join("token"), values: Mutex::new(values), token: Mutex::new(None) }
    }

    pub fn get(&self) -> Values {
        self.values.lock().unwrap().clone()
    }

    /// Changes and saves at once, through a temporary file, so a crash
    /// mid-write never leaves half a file.
    pub fn update(&self, f: impl FnOnce(&mut Values)) {
        let mut v = self.values.lock().unwrap();
        f(&mut v);
        let tmp = self.file.with_extension("json.tmp");
        if let Ok(text) = serde_json::to_vec_pretty(&*v) {
            if fs::write(&tmp, text).is_ok() {
                let _ = fs::rename(&tmp, &self.file);
            }
        }
    }

    /// Forgets the account's cursors and state; the server address and this computer's limits stay.
    pub fn forget_account(&self) {
        self.update(|v| {
            *v = Values {
                server: std::mem::take(&mut v.server),
                cache_limit_gb: v.cache_limit_gb,
                download_limit_gb: v.download_limit_gb,
                download_on_metered: v.download_on_metered,
                speed: v.speed,
                semitones: v.semitones,
                sleep_finish_song: v.sleep_finish_song,
                theme: std::mem::take(&mut v.theme),
                palette: v.palette,
                ..Default::default()
            }
        });
    }

    /// The sign-in token, in a file only this user can read, as the phone
    /// keeps it in its app-private storage and the Mac in its own folder
    /// (plan 039).
    pub fn token(&self) -> String {
        let mut t = self.token.lock().unwrap();
        t.get_or_insert_with(|| fs::read_to_string(&self.token_file).unwrap_or_default().trim().to_string()).clone()
    }

    pub fn set_token(&self, token: &str) {
        let mut t = self.token.lock().unwrap();
        *t = Some(token.to_string());
        if token.is_empty() {
            let _ = fs::remove_file(&self.token_file);
            return;
        }
        let tmp = self.token_file.with_extension("tmp");
        let written =
            fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp).and_then(|mut f| {
                f.set_permissions(fs::Permissions::from_mode(0o600))?;
                f.write_all(token.as_bytes())
            });
        if written.is_ok() {
            let _ = fs::rename(&tmp, &self.token_file);
        }
    }
}

/// "nas" → "http://nas:8585", as the phone reads what was typed. Only plain
/// HTTP gets the default port: HTTPS is `tailscale serve`, on 443.
pub fn server_url(typed: &str) -> String {
    let mut s = typed.trim().trim_end_matches('/').to_string();
    if !s.contains("://") {
        s = format!("http://{s}");
    }
    let (scheme, rest) = s.split_once("://").unwrap();
    let host = rest.split('/').next().unwrap_or("");
    if !host.contains(':') && scheme == "http" {
        return format!("http://{host}:8585{}", &rest[host.len()..]);
    }
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_server_address_gets_a_scheme_and_the_default_port() {
        assert_eq!(super::server_url(" my-nas "), "http://my-nas:8585");
        assert_eq!(super::server_url("192.168.1.50:8585/"), "http://192.168.1.50:8585");
        assert_eq!(super::server_url("https://nas.example.ts.net"), "https://nas.example.ts.net");
    }

    // The token is a credential: no other user on the machine may read it.
    #[test]
    fn the_token_file_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("dhun-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = super::Prefs::open(&dir);
        p.set_token("secret");
        let mode = std::fs::metadata(dir.join("token")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(super::Prefs::open(&dir).token(), "secret");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
