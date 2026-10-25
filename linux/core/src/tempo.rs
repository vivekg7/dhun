//! Play speed and pitch (plan 016), independent of each other: a faster
//! speed keeps the voice's pitch, and the pitch moves in semitones, the
//! steps a singer thinks in. As the Mac's `Tempo`.

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tempo {
    pub speed: f64,
    pub semitones: i64,
}

impl Default for Tempo {
    fn default() -> Tempo {
        Tempo::NORMAL
    }
}

impl Tempo {
    pub const NORMAL: Tempo = Tempo { speed: 1.0, semitones: 0 };
    pub const MIN_SPEED: f64 = 0.5;
    pub const MAX_SPEED: f64 = 2.0;
    pub const MAX_SEMITONES: i64 = 6;

    pub fn json(self) -> Value {
        json!({ "speed": self.speed, "semitones": self.semitones })
    }

    /// What the button on Now playing shows, or None when normal: the
    /// speed ("1.5×"), else the pitch ("+2").
    pub fn label(self) -> Option<String> {
        if self.speed != 1.0 {
            Some(format!("{}×", format(self.speed)))
        } else if self.semitones > 0 {
            Some(format!("+{}", self.semitones))
        } else if self.semitones < 0 {
            Some(format!("−{}", -self.semitones))
        } else {
            None
        }
    }

    /// A song's setting (`speed.<id>`), or None when it has none or it
    /// cannot be read. It comes from another device, maybe a newer app:
    /// never trusted to be in range.
    pub fn of(v: Option<&Value>) -> Option<Tempo> {
        let o = v?.as_object()?;
        let speed = o.get("speed").and_then(Value::as_f64).unwrap_or(1.0);
        let semitones = o.get("semitones").and_then(Value::as_f64).map_or(0, |s| s.round() as i64);
        Some(Tempo {
            speed: speed.clamp(Tempo::MIN_SPEED, Tempo::MAX_SPEED),
            semitones: semitones.clamp(-Tempo::MAX_SEMITONES, Tempo::MAX_SEMITONES),
        })
    }
}

pub fn song_setting(song: i64) -> String {
    format!("speed.{song}")
}

/// Every podcast's or every audiobook's: `speed.podcast`, `speed.audiobook`.
pub fn kind_setting(kind: &str) -> String {
    format!("speed.{kind}")
}

/// 1.25 → "1.25", 1.5 → "1.5", 2 → "2".
pub fn format(speed: f64) -> String {
    let hundredths = (speed * 100.0).round() as i64;
    if hundredths % 100 == 0 {
        return (hundredths / 100).to_string();
    }
    format!("{}.{:02}", hundredths / 100, hundredths % 100).trim_end_matches('0').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_song_setting_is_read_defensively() {
        let t = |s: &str| Tempo::of(Some(&serde_json::from_str(s).unwrap()));
        assert_eq!(t(r#"{"speed":1.5,"semitones":-2}"#), Some(Tempo { speed: 1.5, semitones: -2 }));
        assert_eq!(t(r#"{"speed":9,"semitones":40}"#), Some(Tempo { speed: 2.0, semitones: 6 }));
        assert_eq!(t(r#"{"speed":1.25}"#), Some(Tempo { speed: 1.25, semitones: 0 }));
        assert_eq!(t("1.5"), None);
        assert_eq!(Tempo::of(None), None);
        assert_eq!([0.75, 1.0, 1.25, 1.5, 2.0].map(format), ["0.75", "1", "1.25", "1.5", "2"]);
    }
}
