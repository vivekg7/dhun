//! The client for api/openapi.yaml, hand-written as the phone's and the
//! Mac's are (plan 006). Blocking: the app calls it from worker threads, so
//! no async runtime joins (plan 039).

use crate::model::{Chapter, Song, User};
use crate::prefs::Prefs;
use crate::time;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub enum Error {
    /// The server answered with an error status.
    Http { code: u16, message: String },
    /// No answer: offline, or the server down.
    Network(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Http { message, .. } => f.write_str(message),
            Error::Network(m) => f.write_str(m),
        }
    }
}

impl Error {
    pub fn code(&self) -> u16 {
        match self {
            Error::Http { code, .. } => *code,
            Error::Network(_) => 0,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Api {
    pub prefs: Arc<Prefs>,
    agent: ureq::Agent,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Login {
    pub token: String,
    pub device_id: Option<i64>,
    pub user: Option<User>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SongDto {
    pub id: i64,
    pub path: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub artists: Option<Vec<String>>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub composer: Option<String>,
    pub genres: Option<Vec<String>>,
    pub year: Option<i64>,
    pub track: Option<i64>,
    pub disc: Option<i64>,
    pub duration_ms: Option<i64>,
    pub format: Option<String>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i64>,
    pub bit_depth: Option<i64>,
    pub size: Option<i64>,
    pub has_art: Option<bool>,
    pub art: Option<String>,
    pub has_lyrics: Option<bool>,
    pub added_at: Option<String>,
    pub missing: Option<bool>,
    // Podcasts and audiobooks only (plan 031); no kind is music.
    pub kind: Option<String>,
    pub date: Option<String>,
    pub notes: Option<String>,
    pub chapters: Option<Vec<Chapter>>,
    pub group: Option<String>,
    pub group_title: Option<String>,
    pub group_index: Option<i64>,
}

impl SongDto {
    pub fn song(self) -> Song {
        let path = self.path.unwrap_or_default();
        let title = match self.title {
            Some(t) if !t.is_empty() => t,
            // An untagged file is named after itself, without its extension.
            _ => {
                let name = path.rsplit('/').next().unwrap_or("");
                name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string()
            }
        };
        Song {
            id: self.id,
            title,
            path,
            artist: self.artist.unwrap_or_default(),
            artists: self.artists.unwrap_or_default(),
            album: self.album.unwrap_or_default(),
            album_artist: self.album_artist.unwrap_or_default(),
            composer: self.composer.unwrap_or_default(),
            genres: self.genres.unwrap_or_default(),
            year: self.year.unwrap_or(0),
            track: self.track.unwrap_or(0),
            disc: self.disc.unwrap_or(0),
            duration_ms: self.duration_ms.unwrap_or(0),
            format: self.format.unwrap_or_default(),
            bitrate: self.bitrate.unwrap_or(0),
            sample_rate: self.sample_rate.unwrap_or(0),
            bit_depth: self.bit_depth.unwrap_or(0),
            size: self.size.unwrap_or(0),
            has_art: self.has_art.unwrap_or(false),
            has_lyrics: self.has_lyrics.unwrap_or(false),
            added_at: time::parse(self.added_at.as_deref().unwrap_or("")),
            missing: self.missing.unwrap_or(false),
            art: self.art.unwrap_or_default(),
            kind: self.kind.unwrap_or_default(),
            date: self.date.unwrap_or_default(),
            notes: self.notes.unwrap_or_default(),
            chapters: self.chapters.unwrap_or_default(),
            group_key: self.group.unwrap_or_default(),
            group_title: self.group_title.unwrap_or_default(),
            group_index: self.group_index.unwrap_or(0),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct PlaylistDto {
    pub id: i64,
    pub deleted: Option<bool>,
    pub path: Option<String>,
    pub name: Option<String>,
    pub shared: Option<bool>,
    pub r#ref: Option<String>,
    pub songs: Option<Vec<i64>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Library {
    pub version: Option<i64>,
    pub songs: Option<Vec<SongDto>>,
    pub playlists: Option<Vec<PlaylistDto>>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Me {
    pub user: Option<User>,
    pub device_id: Option<i64>,
}

#[derive(Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Thumbs {
    thumbs: Option<HashMap<String, String>>,
}

impl Api {
    pub fn new(prefs: Arc<Prefs>) -> Api {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();
        Api { prefs, agent }
    }

    pub fn server(&self) -> String {
        self.prefs.get().server
    }

    pub fn stream_url(&self, song: i64) -> String {
        format!("{}/api/v1/stream/{song}", self.server())
    }

    pub fn login(&self, server: &str, user: &str, password: &str, device: &str) -> Result<Login> {
        let body = serde_json::json!({ "username": user, "password": password, "device": device });
        self.call("POST", &format!("{server}/api/v1/login"), Some(body), false)
    }

    /// Removes this device's token on the server.
    pub fn logout(&self) -> Result<()> {
        let url = format!("{}/api/v1/logout", self.server());
        self.send("POST", &url, None, true).map(|_| ())
    }

    pub fn me(&self) -> Result<Me> {
        self.call("GET", "/api/v1/me", None, true)
    }

    /// Podcasts and audiobooks too (`kinds=all`): without it the server sends music only, for apps that predate them.
    pub fn library(&self, since: i64) -> Result<Library> {
        self.call("GET", &format!("/api/v1/library?since={since}&kinds=all"), None, true)
    }

    /// Thumbnails by art key, base64 JPEG.
    pub fn thumbs(&self, keys: &[String]) -> Result<HashMap<String, String>> {
        let r: Thumbs = self.call("POST", "/api/v1/thumbs", Some(serde_json::json!({ "keys": keys })), true)?;
        Ok(r.thumbs.unwrap_or_default())
    }

    /// A cover at `size` px; None when the song has none.
    pub fn art(&self, song: i64, size: u32) -> Result<Option<Vec<u8>>> {
        let url = format!("{}/api/v1/art/{song}?size={size}", self.server());
        let mut resp = self.send("GET", &url, None, true)?;
        if resp.status() != 200 {
            return Ok(None);
        }
        resp.body_mut().with_config().limit(16 << 20).read_to_vec().map(Some).map_err(|e| Error::Network(e.to_string()))
    }

    fn send(
        &self,
        method: &str,
        url: &str,
        body: Option<serde_json::Value>,
        auth: bool,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        if url.starts_with("https://") {
            // Plain HTTP only for now, the NAS's default (plan 039).
            return Err(Error::Network("This build cannot reach an https:// server yet".into()));
        }
        let token = if auth { self.prefs.token() } else { String::new() };
        let bearer = format!("Bearer {token}");
        let result = if method == "GET" {
            let r = self.agent.get(url);
            if token.is_empty() { r.call() } else { r.header("Authorization", &bearer).call() }
        } else {
            let mut r = self.agent.post(url).header("Content-Type", "application/json");
            if !token.is_empty() {
                r = r.header("Authorization", &bearer);
            }
            r.send(body.map_or_else(String::new, |b| b.to_string()))
        };
        let resp = result.map_err(|e| Error::Network(e.to_string()))?;
        if auth {
            // The server's release, for Settings; sent only to a signed-in device.
            if let Some(v) = resp.headers().get("Dhun-Version").and_then(|v| v.to_str().ok()) {
                if v != self.prefs.get().server_version {
                    let v = v.to_string();
                    self.prefs.update(|p| p.server_version = v);
                }
            }
        }
        Ok(resp)
    }

    fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
        auth: bool,
    ) -> Result<T> {
        let url = if path.starts_with("http") { path.to_string() } else { format!("{}{path}", self.server()) };
        let mut resp = self.send(method, &url, body, auth)?;
        let status = resp.status().as_u16();
        // Read as it arrives: the first library pull is tens of megabytes of JSON.
        let reader = resp.body_mut().as_reader();
        if !(200..300).contains(&status) {
            let msg = serde_json::from_reader::<_, ErrorBody>(reader).ok().and_then(|b| b.message);
            return Err(Error::Http { code: status, message: msg.unwrap_or_else(|| format!("HTTP {status}")) });
        }
        serde_json::from_reader(std::io::BufReader::with_capacity(1 << 16, reader))
            .map_err(|e| Error::Network(e.to_string()))
    }
}
