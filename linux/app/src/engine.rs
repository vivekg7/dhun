//! The player underneath `Playback`: GStreamer's `playbin3`, fed by a
//! source element of our own that reads a song's bytes as they arrive
//! (docs/plans/039_linux_app.md, "What the spike showed"). Its control
//! calls are made on the main thread; GStreamer's own threads call
//! `about-to-finish` and the source, which only touch `Shared`.

use dhun_core::catalog::SongRef;
use dhun_core::queue::Upcoming;
use dhun_core::stream::Part;
use gst::glib;
use gst::prelude::*;
use gstreamer as gst;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// What the player tells `Playback`, on the main thread.
#[derive(Debug)]
pub enum Event {
    /// A song began to be heard: the one asked for, or the next by itself.
    Started(i64),
    /// Everything scheduled has played.
    Ended,
    Failed(i64, String),
    /// The output went away (headphones, a Bluetooth device): the player stopped.
    OutputChanged,
}

/// A song ready to open: its bytes and its container.
pub struct Source {
    pub part: Arc<Part>,
    pub format: String,
}

type Opener = Box<dyn Fn(&SongRef) -> Option<Source> + Send + Sync>;

struct Shared {
    upcoming: Mutex<Upcoming>,
    /// The queue's songs by id, for opening the next one on GStreamer's thread.
    songs: Mutex<HashMap<i64, SongRef>>,
    open: Opener,
    /// Sources waiting for `source-setup`, by URI.
    pending: Mutex<HashMap<String, Source>>,
    /// The song last handed to playbin: what it decodes now or next.
    decoding: AtomicU64,
    serial: AtomicU64,
    /// The container of the source just set up, for the type finder made next.
    caps: Mutex<Option<gst::Caps>>,
}

pub struct Engine {
    playbin: gst::Element,
    shared: Arc<Shared>,
    /// Where to seek once the song opened by `play` has prerolled.
    pending_seek: std::cell::Cell<Option<(u64, bool)>>,
    _bus: gst::bus::BusWatchGuard,
}

impl Engine {
    pub fn new(open: Opener, on_event: impl Fn(Event) + 'static) -> Engine {
        sparse::SparseSrc::register();
        let playbin = gst::ElementFactory::make("playbin3").build().expect("GStreamer's playbin3");
        // DHUN_AUDIO_SINK replaces the output, as "fakesink sync=true" for a test without sound.
        let sink = match std::env::var("DHUN_AUDIO_SINK") {
            Ok(desc) => gst::parse::bin_from_description(&desc, true).ok().map(|b| b.upcast::<gst::Element>()),
            Err(_) => gst::ElementFactory::make("pulsesink").build().ok(),
        };
        if let Some(sink) = &sink {
            playbin.set_property("audio-sink", sink);
        }
        let shared = Arc::new(Shared {
            upcoming: Mutex::default(),
            songs: Mutex::default(),
            open,
            pending: Mutex::default(),
            decoding: AtomicU64::new(0),
            serial: AtomicU64::new(0),
            caps: Mutex::default(),
        });
        let s = shared.clone();
        playbin.connect("source-setup", false, move |args| {
            let src = args[1].get::<gst::Element>().ok()?;
            let uri = src.dynamic_cast_ref::<gst::URIHandler>()?.uri()?.to_string();
            let source = s.pending.lock().unwrap().remove(&uri)?;
            *s.caps.lock().unwrap() = container(&source.format);
            if let Ok(own) = src.downcast::<sparse::SparseSrc>() {
                own.set(source);
            }
            None
        });
        // The server knows each song's container, so type detection, which
        // probes the middle of a file for MP3, is skipped (plan 039).
        let s = shared.clone();
        playbin.connect("element-setup", false, move |args| {
            let e = args[1].get::<gst::Element>().ok()?;
            if e.factory().is_some_and(|f| f.name() == "typefind") {
                if let Some(c) = s.caps.lock().unwrap().take() {
                    e.set_property("force-caps", &c);
                }
            }
            None
        });
        // Gapless: the next song is handed over while this one still decodes.
        let s = shared.clone();
        playbin.connect("about-to-finish", false, move |args| {
            let pb = args[0].get::<gst::Element>().ok()?;
            let after = s.decoding.load(Ordering::SeqCst) as i64;
            let next = s.upcoming.lock().unwrap().next(after)?;
            let song = s.songs.lock().unwrap().get(&next).cloned()?;
            let source = (s.open)(&song)?;
            let uri = s.register(next, source);
            pb.set_property("uri", &uri);
            None
        });
        if let Some(sink) = sink.as_ref().filter(|s| s.find_property("current-device").is_some()) {
            // Pulse moved the stream to another device: the one heard went
            // away (headphones, Bluetooth), as the phone's "becoming noisy".
            // Said on the bus, which the main thread reads; the first device
            // is the stream's start, not a change.
            let last: Mutex<Option<String>> = Mutex::default();
            sink.connect_notify(Some("current-device"), move |sink, _| {
                let now = sink.property::<Option<String>>("current-device");
                let before = std::mem::replace(&mut *last.lock().unwrap(), now.clone());
                if before.is_some() && before != now {
                    let s = gst::Structure::new_empty("dhun-output-changed");
                    let _ = sink.post_message(gst::message::Element::builder(s).src(sink).build());
                }
            });
        }
        let pb = playbin.downgrade();
        let s = shared.clone();
        let bus = playbin.bus().unwrap();
        let guard = bus
            .add_watch_local(move |_, msg| {
                let Some(pb) = pb.upgrade() else { return glib::ControlFlow::Break };
                match msg.view() {
                    gst::MessageView::StreamStart(_) => {
                        let uri = pb.property::<Option<String>>("current-uri").unwrap_or_default();
                        if let Some(song) = song_of(&uri) {
                            on_event(Event::Started(song));
                        }
                    }
                    gst::MessageView::Eos(_) => on_event(Event::Ended),
                    gst::MessageView::Error(e) => {
                        let song = s.decoding.load(Ordering::SeqCst) as i64;
                        on_event(Event::Failed(song, e.error().to_string()));
                    }
                    gst::MessageView::Element(e)
                        if e.structure().is_some_and(|s| s.name() == "dhun-output-changed") =>
                    {
                        on_event(Event::OutputChanged)
                    }
                    _ => {}
                }
                glib::ControlFlow::Continue
            })
            .expect("a bus watch");
        Engine { playbin, shared, pending_seek: Default::default(), _bus: guard }
    }

    /// The queue's songs and play order, for what follows by itself.
    pub fn set_upcoming(&self, up: Upcoming, songs: HashMap<i64, SongRef>) {
        *self.shared.upcoming.lock().unwrap() = up;
        *self.shared.songs.lock().unwrap() = songs;
    }

    /// Opens `source` for `song` at `from_ms`, playing or paused.
    pub fn play(&self, song: i64, source: Source, from_ms: i64, paused: bool) {
        let _ = self.playbin.set_state(gst::State::Ready);
        let uri = self.shared.register(song, source);
        self.playbin.set_property("uri", &uri);
        self.pending_seek.set((from_ms > 0).then_some((from_ms as u64, paused)));
        let state = if paused || from_ms > 0 { gst::State::Paused } else { gst::State::Playing };
        let _ = self.playbin.set_state(state);
        if from_ms > 0 {
            self.seek_when_ready();
        }
    }

    /// The seek `play` asked for, once the song has prerolled: a seek
    /// before it would be lost.
    fn seek_when_ready(&self) {
        let pb = self.playbin.downgrade();
        let pending = self.pending_seek.take();
        let Some((ms, paused)) = pending else { return };
        glib::timeout_add_local(std::time::Duration::from_millis(30), move || {
            let Some(pb) = pb.upgrade() else { return glib::ControlFlow::Break };
            let (r, now, _) = pb.state(gst::ClockTime::ZERO);
            if r.is_err() {
                return glib::ControlFlow::Break;
            }
            if now < gst::State::Paused {
                return glib::ControlFlow::Continue;
            }
            let _ = pb.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, gst::ClockTime::from_mseconds(ms));
            if !paused {
                let _ = pb.set_state(gst::State::Playing);
            }
            glib::ControlFlow::Break
        });
    }

    pub fn pause(&self) {
        let _ = self.playbin.set_state(gst::State::Paused);
    }

    pub fn resume(&self) {
        let _ = self.playbin.set_state(gst::State::Playing);
    }

    pub fn seek(&self, ms: i64) {
        let _ = self.playbin.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
            gst::ClockTime::from_mseconds(ms.max(0) as u64),
        );
    }

    pub fn stop(&self) {
        let _ = self.playbin.set_state(gst::State::Null);
        self.shared.pending.lock().unwrap().clear();
    }

    /// Where in the song heard now, and how long it is, in milliseconds.
    pub fn position(&self) -> Option<(i64, i64)> {
        let pos = self.playbin.query_position::<gst::ClockTime>()?;
        let dur = self.playbin.query_duration::<gst::ClockTime>().map_or(0, |d| d.mseconds() as i64);
        Some((pos.mseconds() as i64, dur))
    }
}

impl Shared {
    /// A URI of its own for each opening, so the same song twice is two sources.
    fn register(&self, song: i64, source: Source) -> String {
        let uri = format!("dhun://{song}/{}", self.serial.fetch_add(1, Ordering::SeqCst));
        self.pending.lock().unwrap().insert(uri.clone(), source);
        self.decoding.store(song as u64, Ordering::SeqCst);
        uri
    }
}

fn song_of(uri: &str) -> Option<i64> {
    uri.strip_prefix("dhun://")?.split('/').next()?.parse().ok()
}

/// What GStreamer calls a container the server names (api/openapi.yaml `format`).
fn container(format: &str) -> Option<gst::Caps> {
    let caps = match format {
        "mp4" => "video/quicktime",
        "mpeg" => "audio/mpeg, mpegversion=(int)1",
        "flac" => "audio/x-flac",
        "ogg" => "application/ogg",
        "webm" => "video/webm",
        "matroska" => "video/x-matroska",
        "wav" => "audio/x-wav",
        "aiff" => "audio/x-aiff",
        _ => return None,
    };
    caps.parse().ok()
}

/// The source element: GStreamer asks for an exact offset and length, and
/// `unlock` wakes a read waiting for the network when a seek or a stop
/// flushes, as the Mac engine's interruptible read. `appsrc` fed from a
/// thread was tried first and raced a seek (plan 039).
mod sparse {
    use super::Source;
    use gst::glib;
    use gst::prelude::*;
    use gst::subclass::prelude::*;
    use gst_base::prelude::*;
    use gst_base::subclass::prelude::*;
    use gstreamer as gst;
    use gstreamer_base as gst_base;
    use std::sync::{LazyLock, Mutex, OnceLock};

    mod imp {
        use super::*;

        #[derive(Default)]
        pub struct SparseSrc {
            pub source: Mutex<Option<std::sync::Arc<dhun_core::stream::Part>>>,
            pub mp3: Mutex<bool>,
            /// Where an MP3's encoder name sits, renamed as it is read (below).
            pub tag_at: OnceLock<Option<u64>>,
            pub uri: Mutex<String>,
        }

        #[glib::object_subclass]
        impl ObjectSubclass for SparseSrc {
            const NAME: &'static str = "DhunSparseSrc";
            type Type = super::SparseSrc;
            type ParentType = gst_base::BaseSrc;
            type Interfaces = (gst::URIHandler,);
        }

        impl ObjectImpl for SparseSrc {
            fn constructed(&self) {
                self.parent_constructed();
                self.obj().set_format(gst::Format::Bytes);
            }
        }
        impl GstObjectImpl for SparseSrc {}

        impl ElementImpl for SparseSrc {
            fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
                static M: LazyLock<gst::subclass::ElementMetadata> = LazyLock::new(|| {
                    gst::subclass::ElementMetadata::new(
                        "Dhun song",
                        "Source/Network",
                        "A song's bytes as they arrive",
                        "Dhun",
                    )
                });
                Some(&M)
            }
            fn pad_templates() -> &'static [gst::PadTemplate] {
                static T: LazyLock<Vec<gst::PadTemplate>> = LazyLock::new(|| {
                    let any = gst::Caps::new_any();
                    vec![gst::PadTemplate::new("src", gst::PadDirection::Src, gst::PadPresence::Always, &any).unwrap()]
                });
                T.as_ref()
            }
        }

        impl URIHandlerImpl for SparseSrc {
            const URI_TYPE: gst::URIType = gst::URIType::Src;
            fn protocols() -> &'static [&'static str] {
                &["dhun"]
            }
            fn uri(&self) -> Option<String> {
                Some(self.uri.lock().unwrap().clone())
            }
            fn set_uri(&self, uri: &str) -> Result<(), glib::Error> {
                *self.uri.lock().unwrap() = uri.into();
                Ok(())
            }
        }

        impl BaseSrcImpl for SparseSrc {
            fn is_seekable(&self) -> bool {
                true
            }
            fn size(&self) -> Option<u64> {
                self.source.lock().unwrap().as_ref().map(|p| p.len)
            }
            fn unlock(&self) -> Result<(), gst::ErrorMessage> {
                if let Some(p) = self.source.lock().unwrap().as_ref() {
                    p.interrupt(true);
                }
                Ok(())
            }
            fn unlock_stop(&self) -> Result<(), gst::ErrorMessage> {
                if let Some(p) = self.source.lock().unwrap().as_ref() {
                    p.interrupt(false);
                }
                Ok(())
            }
            fn fill(
                &self,
                offset: u64,
                length: u32,
                buffer: &mut gst::BufferRef,
            ) -> Result<gst::FlowSuccess, gst::FlowError> {
                let part = self.source.lock().unwrap().clone().ok_or(gst::FlowError::NotNegotiated)?;
                if offset >= part.len {
                    return Err(gst::FlowError::Eos);
                }
                let mut bytes = part.read(offset, length as usize).ok_or(gst::FlowError::Flushing)?;
                if *self.mp3.lock().unwrap() {
                    let at = match self.tag_at.get() {
                        Some(at) => *at,
                        None => {
                            let at = lame_tag_at(&part).ok_or(gst::FlowError::Flushing)?;
                            *self.tag_at.get_or_init(|| at)
                        }
                    };
                    // FFmpeg writes LAME's tag under its own name, which GStreamer's
                    // parser does not read, so its gapless trim is lost (plan 039).
                    if let Some(at) = at.filter(|&a| a + 4 > offset && a < offset + bytes.len() as u64) {
                        for (i, b) in b"LAME".iter().enumerate() {
                            let p = at + i as u64;
                            if p >= offset && p < offset + bytes.len() as u64 {
                                bytes[(p - offset) as usize] = *b;
                            }
                        }
                    }
                }
                buffer.set_size(bytes.len());
                buffer.map_writable().map_err(|_| gst::FlowError::Error)?.copy_from_slice(&bytes);
                buffer.set_offset(offset);
                Ok(gst::FlowSuccess::Ok)
            }
        }

        /// Where FFmpeg's encoder name ("Lavc", "Lavf") sits in an MP3's
        /// Xing or Info frame: Some(None) for a file without one, None when
        /// a flush interrupted the reading.
        fn lame_tag_at(part: &dhun_core::stream::Part) -> Option<Option<u64>> {
            let head = part.read(0, 10)?;
            let mut start = 0u64;
            if head.len() == 10 && &head[..3] == b"ID3" {
                let size = head[6..10].iter().fold(0u64, |a, &b| (a << 7) | u64::from(b & 0x7f));
                start = 10 + size + if head[5] & 0x10 != 0 { 10 } else { 0 };
            }
            let frame = part.read(start, 512)?;
            let marker = frame.windows(4).position(|w| w == b"Xing" || w == b"Info");
            Some(marker.and_then(|m| {
                let near = &frame[m..frame.len().min(m + 200)];
                near.windows(4).position(|w| w == b"Lavc" || w == b"Lavf").map(|i| start + (m + i) as u64)
            }))
        }
    }

    glib::wrapper! {
        pub struct SparseSrc(ObjectSubclass<imp::SparseSrc>) @extends gst_base::BaseSrc, gst::Element, gst::Object, @implements gst::URIHandler;
    }

    impl SparseSrc {
        pub fn register() {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| {
                gst::Element::register(None, "dhunsrc", gst::Rank::PRIMARY, Self::static_type()).unwrap();
            });
        }

        pub fn set(&self, s: Source) {
            *self.imp().mp3.lock().unwrap() = s.format == "mpeg";
            *self.imp().source.lock().unwrap() = Some(s.part);
        }
    }
}
