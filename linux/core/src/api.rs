//! The client for api/openapi.yaml, hand-written as the phone's and the
//! Mac's are (plan 006). Blocking: the app calls it from worker threads, so
//! no async runtime joins (plan 039).

use crate::model::{Chapter, NowPlaying, Song, User};
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
    /// For songs' bytes: each request has a bounded window and a deadline,
    /// so a stalled connection is given up and asked again (plan 019).
    stream: ureq::Agent,
}

/// What one request for a song's bytes asks for at most.
pub const RANGE_WINDOW: u64 = 2 << 20;

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
            added_at: self.added_at.as_deref().map_or(0, time::parse),
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

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct QueueDto {
    pub id: String,
    pub deleted: Option<bool>,
    pub name: Option<String>,
    pub songs: Option<Vec<i64>>,
    pub current_song: Option<i64>,
    pub position_ms: Option<i64>,
    pub shuffle: Option<bool>,
    pub repeat: Option<String>,
    pub used_at: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ListItemDto {
    pub song: i64,
    pub deleted: Option<bool>,
    pub at: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ResumeDto {
    pub song: i64,
    pub deleted: Option<bool>,
    pub position_ms: Option<i64>,
    pub at: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LyricsBody {
    pub text: Option<String>,
}

/// A family member, as the admin sees them (plan 023).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Member {
    pub id: i64,
    pub name: String,
    pub admin: bool,
    pub devices: i64,
    pub last_seen_at: String,
}

#[derive(Deserialize)]
struct Members {
    users: Option<Vec<Member>>,
}

#[derive(Deserialize)]
struct LyricsHits {
    hits: Option<Vec<crate::lyrics::Hit>>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BookmarkDto {
    pub id: String,
    pub deleted: Option<bool>,
    pub song: Option<i64>,
    pub position_ms: Option<i64>,
    pub note: Option<String>,
    pub at: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct OpResult {
    pub id: String,
    pub status: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncState {
    pub version: Option<i64>,
    pub queues: Option<Vec<QueueDto>>,
    pub favorites: Option<Vec<ListItemDto>>,
    pub listen_later: Option<Vec<ListItemDto>>,
    pub resume: Option<Vec<ResumeDto>>,
    pub played: Option<Vec<ListItemDto>>,
    pub bookmarks: Option<Vec<BookmarkDto>>,
    pub settings: Option<HashMap<String, serde_json::Value>>,
    pub now_playing: Option<NowPlaying>,
    pub results: Option<Vec<OpResult>>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlayDto {
    pub song: i64,
    pub count: Option<i64>,
    pub last_played_at: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Plays {
    plays: Option<Vec<PlayDto>>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct NowPlayingBody {
    now_playing: Option<NowPlaying>,
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
        let stream = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(15)))
            .timeout_recv_body(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .build()
            .into();
        Api { prefs, agent, stream }
    }

    pub fn server(&self) -> String {
        self.prefs.get().server
    }

    pub fn stream_url(&self, song: i64) -> String {
        format!("{}/api/v1/stream/{song}", self.server())
    }

    /// The bytes of a stream from `from`, at most `RANGE_WINDOW` of them.
    pub fn range(&self, url: &str, from: u64) -> Result<impl std::io::Read + 'static> {
        let token = self.prefs.token();
        let resp = self
            .stream
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Range", format!("bytes={from}-{}", from + RANGE_WINDOW - 1))
            .call()
            .map_err(|e| Error::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        if status != 206 && !(status == 200 && from == 0) {
            return Err(Error::Http { code: status, message: format!("HTTP {status}") });
        }
        Ok(resp.into_body().into_reader())
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

    /// Pushes ops and pulls what changed since the cursor (plan 006).
    pub fn sync(&self, since: i64, ops: Vec<serde_json::Value>) -> Result<SyncState> {
        self.call("POST", "/api/v1/sync", Some(serde_json::json!({ "since": since, "ops": ops })), true)
    }

    /// Every song this user played, with its count.
    pub fn plays(&self) -> Result<Vec<PlayDto>> {
        let r: Plays = self.call("GET", "/api/v1/plays", None, true)?;
        Ok(r.plays.unwrap_or_default())
    }

    pub fn now_playing(&self) -> Result<Option<NowPlaying>> {
        let r: NowPlayingBody = self.call("GET", "/api/v1/now-playing", None, true)?;
        Ok(r.now_playing)
    }

    /// A song's lyrics as text, LRC or plain; 404 when the server has none.
    pub fn lyrics(&self, song: i64) -> Result<String> {
        let r: LyricsBody = self.call("GET", &format!("/api/v1/lyrics/{song}"), None, true)?;
        Ok(r.text.unwrap_or_default())
    }

    /// Songs whose lyrics hold the words, with the line (plan 029).
    pub fn search_lyrics(&self, q: &str) -> Result<Vec<crate::lyrics::Hit>> {
        let encoded: String = q
            .bytes()
            .map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") })
            .collect();
        let r: LyricsHits = self.call("GET", &format!("/api/v1/search/lyrics?q={encoded}"), None, true)?;
        Ok(r.hits.unwrap_or_default())
    }

    // Family members, for the admin (plan 023).

    pub fn members(&self) -> Result<Vec<Member>> {
        let r: Members = self.call("GET", "/api/v1/admin/users", None, true)?;
        Ok(r.users.unwrap_or_default())
    }

    pub fn add_member(&self, name: &str, password: &str) -> Result<()> {
        let body = serde_json::json!({ "name": name, "password": password });
        self.call_done("POST", "/api/v1/admin/users", body)
    }

    pub fn reset_password(&self, user: i64, password: &str) -> Result<()> {
        let body = serde_json::json!({ "password": password });
        self.call_done("PUT", &format!("/api/v1/admin/users/{user}/password"), body)
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
            let r = if method == "PUT" { self.agent.put(url) } else { self.agent.post(url) };
            let mut r = r.header("Content-Type", "application/json");
            if !token.is_empty() {
                r = r.header("Authorization", &bearer);
            }
            r.send(body.map_or_else(String::new, |b| b.to_string()))
        };
        let resp = result.map_err(|e| Error::Network(e.to_string()))?;
        // The server's release, for Settings; sent only to a signed-in device.
        let version = resp.headers().get("Dhun-Version").and_then(|v| v.to_str().ok()).filter(|_| auth);
        if let Some(v) = version.filter(|v| *v != self.prefs.get().server_version) {
            self.prefs.update(|p| p.server_version = v.to_string());
        }
        Ok(resp)
    }

    /// A call whose answer says only that it was done, perhaps with no body.
    fn call_done(&self, method: &str, path: &str, body: serde_json::Value) -> Result<()> {
        let mut resp = self.send(method, &format!("{}{path}", self.server()), Some(body), true)?;
        let status = resp.status().as_u16();
        if (200..300).contains(&status) { Ok(()) } else { Err(http_error(status, resp.body_mut().as_reader())) }
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
            return Err(http_error(status, reader));
        }
        serde_json::from_reader(std::io::BufReader::with_capacity(1 << 16, reader))
            .map_err(|e| Error::Network(e.to_string()))
    }
}

/// An error status, with the server's message when its body has one.
fn http_error(code: u16, body: impl std::io::Read) -> Error {
    let message = serde_json::from_reader::<_, ErrorBody>(body).ok().and_then(|b| b.message);
    Error::Http { code, message: message.unwrap_or_else(|| format!("HTTP {code}")) }
}
