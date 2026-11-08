// SPDX-License-Identifier: GPL-3.0-or-later
// Shared by the standalone verifier and the module compiled into Horizon.
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    sync::{OnceLock, RwLock},
    time::{Duration, Instant},
};

pub const ID: &str = "viahorizon";
// Read only when migrating a previous installation.
pub const LEGACY_ID: &str = "viafabricplus";
pub const NAME: &str = "ViaHorizon";
pub const VERSION: &str = "0.8.8-dev";
pub const CLIENT_PROTOCOL: i32 = 777;
pub const CLIENT_VERSION: &str = "26.3";
pub const MAX_PACKET: usize = 2_097_152;
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Native,
    Gameplay,
    ConfigurationPreview,
    Planned,
}
impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Gameplay => "gameplay",
            Self::ConfigurationPreview => "configuration preview",
            Self::Planned => "not implemented",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Release {
    pub first: &'static str,
    pub last: &'static str,
    pub protocol: i32,
    pub legacy: bool,
    pub stage: Stage,
}
impl Release {
    pub fn label(&self) -> String {
        if self.first == self.last {
            self.first.into()
        } else {
            format!("{}-{}", self.first, self.last)
        }
    }
}

// Protocol identifiers and release ranges from ViaVersion and ViaLegacy.
// This is a target catalog, NOT a claim of implemented translators.
pub const RELEASES: &[Release] = &[
    Release {
        first: "1.0.0",
        last: "1.0.1",
        protocol: 22,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.1",
        last: "1.1",
        protocol: 23,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.2.1",
        last: "1.2.3",
        protocol: 28,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.2.4",
        last: "1.2.5",
        protocol: 29,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.3.1",
        last: "1.3.2",
        protocol: 39,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.4.2",
        last: "1.4.2",
        protocol: 47,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.4.4",
        last: "1.4.5",
        protocol: 49,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.4.6",
        last: "1.4.7",
        protocol: 51,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.5",
        last: "1.5.1",
        protocol: 60,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.5.2",
        last: "1.5.2",
        protocol: 61,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.6.1",
        last: "1.6.1",
        protocol: 73,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.6.2",
        last: "1.6.2",
        protocol: 74,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.6.4",
        last: "1.6.4",
        protocol: 78,
        legacy: true,
        stage: Stage::Planned,
    },
    Release {
        first: "1.7.2",
        last: "1.7.5",
        protocol: 4,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.7.6",
        last: "1.7.10",
        protocol: 5,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.8",
        last: "1.8.9",
        protocol: 47,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.9",
        last: "1.9",
        protocol: 107,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.9.1",
        last: "1.9.1",
        protocol: 108,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.9.2",
        last: "1.9.2",
        protocol: 109,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.9.3",
        last: "1.9.4",
        protocol: 110,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.10",
        last: "1.10.2",
        protocol: 210,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.11",
        last: "1.11",
        protocol: 315,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.11.1",
        last: "1.11.2",
        protocol: 316,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.12",
        last: "1.12",
        protocol: 335,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.12.1",
        last: "1.12.1",
        protocol: 338,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.12.2",
        last: "1.12.2",
        protocol: 340,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.13",
        last: "1.13",
        protocol: 393,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.13.1",
        last: "1.13.1",
        protocol: 401,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.13.2",
        last: "1.13.2",
        protocol: 404,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.14",
        last: "1.14",
        protocol: 477,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.14.1",
        last: "1.14.1",
        protocol: 480,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.14.2",
        last: "1.14.2",
        protocol: 485,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.14.3",
        last: "1.14.3",
        protocol: 490,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.14.4",
        last: "1.14.4",
        protocol: 498,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.15",
        last: "1.15",
        protocol: 573,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.15.1",
        last: "1.15.1",
        protocol: 575,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.15.2",
        last: "1.15.2",
        protocol: 578,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.16",
        last: "1.16",
        protocol: 735,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.16.1",
        last: "1.16.1",
        protocol: 736,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.16.2",
        last: "1.16.2",
        protocol: 751,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.16.3",
        last: "1.16.3",
        protocol: 753,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.16.4",
        last: "1.16.5",
        protocol: 754,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.17",
        last: "1.17",
        protocol: 755,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.17.1",
        last: "1.17.1",
        protocol: 756,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.18",
        last: "1.18.1",
        protocol: 757,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.18.2",
        last: "1.18.2",
        protocol: 758,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.19",
        last: "1.19",
        protocol: 759,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.19.1",
        last: "1.19.2",
        protocol: 760,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.19.3",
        last: "1.19.3",
        protocol: 761,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.19.4",
        last: "1.19.4",
        protocol: 762,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.20",
        last: "1.20.1",
        protocol: 763,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.20.2",
        last: "1.20.2",
        protocol: 764,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.20.3",
        last: "1.20.4",
        protocol: 765,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.20.5",
        last: "1.20.6",
        protocol: 766,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.21",
        last: "1.21.1",
        protocol: 767,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.21.2",
        last: "1.21.3",
        protocol: 768,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.21.4",
        last: "1.21.4",
        protocol: 769,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.21.5",
        last: "1.21.5",
        protocol: 770,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.21.6",
        last: "1.21.6",
        protocol: 771,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.21.7",
        last: "1.21.8",
        protocol: 772,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.21.9",
        last: "1.21.10",
        protocol: 773,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "1.21.11",
        last: "1.21.11",
        protocol: 774,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "26.1",
        last: "26.1.2",
        protocol: 775,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "26.2",
        last: "26.2",
        protocol: 776,
        legacy: false,
        stage: Stage::Gameplay,
    },
    Release {
        first: "26.3",
        last: "26.3",
        protocol: 777,
        legacy: false,
        stage: Stage::Native,
    },
];

fn version_tuple(s: &str) -> Option<[u16; 3]> {
    let mut out = [0; 3];
    let parts: Vec<_> = s.split('.').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    for (i, p) in parts.iter().enumerate() {
        out[i] = p.parse().ok()?;
    }
    Some(out)
}
pub fn release(name: &str) -> Option<&'static Release> {
    let value = version_tuple(name)?;
    RELEASES.iter().find(|r| {
        version_tuple(r.first).unwrap() <= value && value <= version_tuple(r.last).unwrap()
    })
}
pub fn modern_protocol(protocol: i32) -> Option<&'static Release> {
    RELEASES
        .iter()
        .find(|r| !r.legacy && r.protocol == protocol)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Selection {
    Native,
    #[default]
    Auto,
    Release(String),
}
impl Selection {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim() {
            "native" => Ok(Self::Native),
            "auto" => Ok(Self::Auto),
            s => release(s)
                .map(|r| Self::Release(r.first.into()))
                .ok_or_else(|| format!("Unknown Release version: {s}")),
        }
    }
    pub fn key(&self) -> &str {
        match self {
            Self::Native => "native",
            Self::Auto => "auto",
            Self::Release(s) => s,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Native => "Native 26.3".into(),
            Self::Auto => "Auto detect".into(),
            Self::Release(s) => {
                let r = release(s).expect("validated selection");
                format!("{} ({})", r.label(), r.stage.label())
            }
        }
    }
    pub fn next(&self) -> Self {
        match self {
            Self::Native => Self::Auto,
            Self::Auto => Self::Release("26.3".into()),
            Self::Release(s) => match RELEASES.iter().position(|r| r.first == s) {
                Some(i) if i > 0 => Self::Release(RELEASES[i - 1].first.into()),
                _ => Self::Native,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub target: Selection,
    pub login_preview: bool,
    pub servers: BTreeMap<String, Selection>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            target: Selection::default(),
            login_preview: false,
            servers: BTreeMap::new(),
        }
    }
}
impl Config {
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > 65_536 {
            return Err("Config exceeds 64 KiB".into());
        }
        let j: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if j["schema"].as_u64() != Some(1) {
            return Err("Unsupported config schema (expected 1)".into());
        }
        let target = Selection::parse(j["target"].as_str().ok_or("Missing target")?)?;
        let login_preview = j["login_preview"]
            .as_bool()
            .ok_or("Missing login_preview")?;
        let mut servers = BTreeMap::new();
        if let Some(entries) = j["servers"].as_object() {
            if entries.len() > 512 {
                return Err("At most 512 server overrides".into());
            }
            for (address, value) in entries {
                servers.insert(
                    server_key(address)?,
                    Selection::parse(value.as_str().ok_or("Invalid server target")?)?,
                );
            }
        } else if !j["servers"].is_null() {
            return Err("servers must be an object".into());
        }
        Ok(Self {
            target,
            login_preview,
            servers,
            ..Self::default()
        })
    }
    pub fn json(&self) -> String {
        let servers: BTreeMap<_, _> = self
            .servers
            .iter()
            .map(|(k, v)| (k.as_str(), v.key()))
            .collect();
        serde_json::to_string_pretty(&serde_json::json!({"schema":1,"target":self.target.key(),"login_preview":self.login_preview,"servers":servers})).unwrap() + "\n"
    }
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::File::open(path) {
            Ok(file) => {
                let mut text = String::new();
                file.take(65_537)
                    .read_to_string(&mut text)
                    .map_err(|e| e.to_string())?;
                Self::parse(&text)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    /// Preserve a new config, or copy legacy settings once without changing the source.
    pub fn load_migrating(path: &Path, previous: &Path) -> Result<(Self, bool)> {
        if path.try_exists().map_err(|e| e.to_string())? {
            return Self::load(path).map(|config| (config, false));
        }
        let config = Self::load(previous)?;
        config.save(path)?;
        Ok((config, true))
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let text = self.json();
        Self::parse(&text)?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension(format!(
            "tmp-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .map_err(|e| e.to_string())?;
            f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            drop(f);
            std::fs::rename(&tmp, path).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(tmp);
        }
        result
    }
    pub fn selection(&self, host: &str, port: u16) -> Selection {
        self.servers
            .get(&endpoint(host, port))
            .cloned()
            .unwrap_or_else(|| self.target.clone())
    }
}
static CONFIG: OnceLock<RwLock<Config>> = OnceLock::new();
static ROUTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn needs_routing() -> bool {
    ROUTING.load(std::sync::atomic::Ordering::Acquire)
}
pub fn configure(config: Config) {
    let routing = config.enabled
        && (config.target != Selection::Native
            || config.servers.values().any(|s| *s != Selection::Native));
    *CONFIG
        .get_or_init(|| RwLock::new(Config::default()))
        .write()
        .unwrap_or_else(|e| e.into_inner()) = config;
    ROUTING.store(routing, std::sync::atomic::Ordering::Release);
}
pub fn configuration() -> Config {
    CONFIG
        .get_or_init(|| RwLock::new(Config::default()))
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

pub fn endpoint(host: &str, port: u16) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}
pub fn address(address: &str) -> Result<(String, u16)> {
    let s = address.trim();
    if s.is_empty() || s.len() > 255 || s.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("Invalid server address".into());
    }
    if let Some(rest) = s.strip_prefix('[') {
        let (host, tail) = rest.split_once(']').ok_or("Unclosed IPv6 address")?;
        host.parse::<std::net::Ipv6Addr>()
            .map_err(|_| "Invalid IPv6 address")?;
        let port = if tail.is_empty() {
            25565
        } else {
            tail.strip_prefix(':')
                .ok_or("Invalid IPv6 port")?
                .parse()
                .map_err(|_| "Invalid port")?
        };
        if port == 0 {
            return Err("Invalid port".into());
        }
        return Ok((host.into(), port));
    }
    if s.contains(':') {
        let (host, port) = s.split_once(':').unwrap();
        let port: u16 = port
            .parse()
            .map_err(|_| "Invalid port (bracket IPv6 addresses)")?;
        if host.is_empty() || port == 0 {
            return Err("Invalid server address".into());
        }
        Ok((host.into(), port))
    } else {
        Ok((s.into(), 25565))
    }
}
pub fn server_key(s: &str) -> Result<String> {
    let (h, p) = address(s)?;
    Ok(endpoint(&h, p))
}

// Strict, bounded wire primitives. Encryption and compression stay in Horizon's
// existing connection; transforms see packet bodies only, never encrypted bytes.
pub struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Cursor<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    pub fn position(&self) -> usize {
        self.pos
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|e| *e <= self.bytes.len())
            .ok_or("Truncated packet")?;
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }
    pub fn varint(&mut self) -> Result<i32> {
        let mut value = 0u32;
        for i in 0..5 {
            let b = self.take(1)?[0];
            if i == 4 && b & 0xF0 != 0 {
                return Err("VarInt overflow".into());
            }
            value |= u32::from(b & 127) << (7 * i);
            if b & 128 == 0 {
                return Ok(value as i32);
            }
        }
        Err("VarInt too long".into())
    }
    pub fn string(&mut self, max: usize) -> Result<String> {
        let n = usize::try_from(self.varint()?).map_err(|_| "Negative string length")?;
        if n > max.saturating_mul(4) {
            return Err("String too long".into());
        }
        let s = std::str::from_utf8(self.take(n)?).map_err(|_| "Invalid UTF-8")?;
        if s.encode_utf16().count() > max {
            return Err("String too long".into());
        }
        Ok(s.into())
    }
    pub fn end(&self) -> Result<()> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err("Trailing packet bytes".into())
        }
    }
}
pub fn put_varint(out: &mut Vec<u8>, value: i32) {
    let mut v = value as u32;
    while v & !127 != 0 {
        out.push((v as u8 & 127) | 128);
        v >>= 7;
    }
    out.push(v as u8);
}
pub fn put_string(out: &mut Vec<u8>, s: &str) {
    put_varint(out, s.len() as i32);
    out.extend_from_slice(s.as_bytes());
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handshake {
    pub protocol: i32,
    pub host: String,
    pub port: u16,
    pub intent: i32,
}
impl Handshake {
    pub fn retarget(mut self, protocol: i32) -> Self {
        self.protocol = protocol;
        // Transfer intention was added in 1.20.5. An older target must see an
        // ordinary login; keep its original virtual hostname and port intact.
        if protocol < 766 && self.intent == 3 {
            self.intent = 2;
        }
        self
    }
    pub fn parse(body: &[u8]) -> Result<Self> {
        let mut r = Cursor::new(body);
        if r.varint()? != 0 {
            return Err("Not a handshake".into());
        }
        let protocol = r.varint()?;
        let host = r.string(255)?;
        let port = u16::from_be_bytes(r.take(2)?.try_into().unwrap());
        let intent = r.varint()?;
        r.end()?;
        if ![1, 2, 3].contains(&intent) {
            return Err("Unknown handshake intent".into());
        }
        Ok(Self {
            protocol,
            host,
            port,
            intent,
        })
    }
    pub fn body(&self) -> Vec<u8> {
        let mut out = vec![0];
        put_varint(&mut out, self.protocol);
        put_string(&mut out, &self.host);
        out.extend_from_slice(&self.port.to_be_bytes());
        put_varint(&mut out, self.intent);
        out
    }
}

pub fn choose(
    selection: &Selection,
    detected: Option<i32>,
    preview: bool,
) -> Result<Option<&'static Release>> {
    let target = match selection {
        Selection::Native => return Ok(None),
        Selection::Auto => modern_protocol(detected.ok_or("Server did not report a protocol")?)
            .ok_or("Server reported an unknown or unsupported Release protocol")?,
        Selection::Release(name) => release(name).ok_or("Invalid stored target")?,
    };
    match target.stage {
        Stage::Native => Ok(None),
        Stage::Gameplay => Ok(Some(target)),
        Stage::ConfigurationPreview if preview => Ok(Some(target)),
        Stage::ConfigurationPreview => Err(format!(
            "ViaHorizon: {} supports configuration preview. Enable Configuration preview to test it; world entry is disabled.",
            target.label()
        )),
        Stage::Planned => Err(format!(
            "ViaHorizon: Release {} is not implemented.",
            target.label()
        )),
    }
}

#[derive(Clone, Debug)]
pub struct LoginProfileKey {
    pub expires_at: i64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct LoginSession {
    pub target: &'static Release,
    finished: bool,
    profile_key: Option<LoginProfileKey>,
}
impl LoginSession {
    pub fn finished(&self) -> bool {
        self.finished
    }
    pub fn new(target: &'static Release) -> Result<Self> {
        if !matches!(target.stage, Stage::ConfigurationPreview | Stage::Gameplay) {
            return Err("No login translator for this target".into());
        }
        Self::for_implementation(target)
    }
    pub(crate) fn for_implementation(target: &'static Release) -> Result<Self> {
        if target.legacy || !(4..=776).contains(&target.protocol) {
            return Err("No implemented login translator for this target".into());
        }
        Ok(Self {
            target,
            finished: false,
            profile_key: None,
        })
    }
    pub fn set_profile_key(&mut self, key: LoginProfileKey) -> Result<()> {
        if self.finished
            || !matches!(self.target.protocol, 759..=760)
            || key.public_key.is_empty()
            || key.public_key.len() > 4096
            || key.signature.is_empty()
            || key.signature.len() > 4096
        {
            return Err("Invalid profile key for the legacy login phase".into());
        }
        self.profile_key = Some(key);
        Ok(())
    }
    pub fn serverbound(&mut self, body: &[u8]) -> Result<Vec<u8>> {
        let mut r = Cursor::new(body);
        let id = r.varint()?;
        if id == 3 {
            r.end()?;
            if !self.finished {
                return Err("Login acknowledgement before login success".into());
            }
            return Err(format!(
                "ViaHorizon: {} login preview succeeded. This login codec stops here; the native session manages configuration. World entry is disabled.",
                self.target.label()
            ));
        }
        if self.finished {
            return Err("Login already finished".into());
        }
        if ![0, 1, 2, 4].contains(&id) {
            return Err(format!("Unsupported login packet {id}"));
        }
        if id == 0 && self.target.protocol < 764 {
            let name = r.string(16)?;
            let uuid = r.take(16)?;
            r.end()?;
            let mut out = vec![0];
            put_string(&mut out, &name);
            if matches!(self.target.protocol, 759..=760) {
                out.push(u8::from(self.profile_key.is_some()));
                if let Some(key) = &self.profile_key {
                    out.extend_from_slice(&key.expires_at.to_be_bytes());
                    for bytes in [&key.public_key, &key.signature] {
                        put_varint(&mut out, bytes.len() as i32);
                        out.extend_from_slice(bytes);
                    }
                }
            }
            if self.target.protocol >= 760 {
                out.push(1);
                out.extend_from_slice(uuid);
            }
            return Ok(out);
        }
        if id == 1 && self.target.protocol < 761 {
            let mut out = vec![1];
            for index in 0..2 {
                let length =
                    usize::try_from(r.varint()?).map_err(|_| "Negative encryption field length")?;
                if length > 4096 {
                    return Err("Encryption field exceeds key bound".into());
                }
                let bytes = r.take(length)?;
                if index == 1 && matches!(self.target.protocol, 759..=760) {
                    out.push(1); // Standard RSA-encrypted nonce branch.
                }
                if self.target.protocol < 47 {
                    out.extend_from_slice(
                        &u16::try_from(length)
                            .map_err(|_| "Old encryption field exceeds short")?
                            .to_be_bytes(),
                    );
                } else {
                    put_varint(&mut out, length as i32);
                }
                out.extend_from_slice(bytes);
            }
            r.end()?;
            return Ok(out);
        }
        Ok(body.to_vec())
    }
    pub fn clientbound(&mut self, id: i32, payload: &[u8]) -> Result<Vec<u8>> {
        if self.finished {
            return Err("Packet after login success".into());
        }
        if !(0..=5).contains(&id) {
            return Err(format!("Unsupported login packet {id}"));
        }
        if (id == 3 && self.target.protocol < 47) || (id == 4 && self.target.protocol < 393) {
            return Err("Login packet did not exist in the selected server version".into());
        }
        let mut out = payload.to_vec();
        if id == 1 && self.target.protocol < 766 {
            let mut r = Cursor::new(payload);
            let server_id = r.string(20)?;
            out.clear();
            put_string(&mut out, &server_id);
            for _ in 0..2 {
                let length = if self.target.protocol < 47 {
                    usize::from(u16::from_be_bytes(r.take(2)?.try_into().unwrap()))
                } else {
                    usize::try_from(r.varint()?).map_err(|_| "Negative encryption field length")?
                };
                if length > 4096 {
                    return Err("Encryption field is too large".into());
                }
                put_varint(&mut out, length as i32);
                out.extend_from_slice(r.take(length)?);
            }
            r.end()?;
            out.push(1); // Older encryption requests always authenticate.
        }
        if id == 2 {
            let mut r = Cursor::new(payload);
            out.clear();
            if self.target.protocol < 735 {
                let value = r.string(36)?;
                let compact = if value.len() == 36
                    && [8, 13, 18, 23]
                        .iter()
                        .all(|&at| value.as_bytes()[at] == b'-')
                {
                    value.chars().filter(|&c| c != '-').collect::<String>()
                } else {
                    value
                };
                if compact.len() != 32 || !compact.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Err("Invalid old login UUID".into());
                }
                let uuid =
                    u128::from_str_radix(&compact, 16).map_err(|_| "Invalid old login UUID")?;
                out.extend_from_slice(&uuid.to_be_bytes());
            } else {
                out.extend_from_slice(r.take(16)?);
            }
            put_string(&mut out, &r.string(16)?);
            let count = if self.target.protocol >= 759 {
                r.varint()?
            } else {
                0
            };
            if !(0..=1024).contains(&count) {
                return Err("Invalid profile property count".into());
            }
            put_varint(&mut out, count);
            for _ in 0..count {
                put_string(&mut out, &r.string(64)?);
                put_string(&mut out, &r.string(32767)?);
                let signed = r.take(1)?[0];
                out.push(signed);
                match signed {
                    0 => (),
                    1 => {
                        put_string(&mut out, &r.string(4096)?);
                    }
                    _ => return Err("Invalid property signature flag".into()),
                }
            }
            if matches!(self.target.protocol, 766..=767) {
                match r.take(1)?[0] {
                    0 | 1 => (),
                    _ => return Err("Invalid strict login error flag".into()),
                }
            }
            if self.target.protocol == 776 {
                out.extend_from_slice(r.take(16)?);
            } // 26.2 introduced a session UUID.
            r.end()?;
            if self.target.protocol < 776 {
                out.extend_from_slice(&[0; 16]);
            } // No secure-chat session is used in a preview.
            self.finished = true;
        }
        Ok(out)
    }
}

pub fn write_frame(stream: &mut impl Write, body: &[u8]) -> Result<()> {
    if body.is_empty() || body.len() > MAX_PACKET {
        return Err("Invalid frame size".into());
    }
    let mut head = Vec::new();
    put_varint(&mut head, body.len() as i32);
    stream
        .write_all(&head)
        .and_then(|()| stream.write_all(body))
        .map_err(|e| e.to_string())
}
pub fn read_frame(stream: &mut impl Read) -> Result<Vec<u8>> {
    let mut n = 0usize;
    for i in 0..3 {
        let mut byte = [0];
        stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
        n |= usize::from(byte[0] & 127) << (7 * i);
        if byte[0] & 128 == 0 {
            if n == 0 || n > MAX_PACKET {
                return Err("Invalid frame size".into());
            }
            let mut out = vec![0; n];
            stream.read_exact(&mut out).map_err(|e| e.to_string())?;
            return Ok(out);
        }
    }
    Err("Frame length exceeds VarInt21".into())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerStatus {
    pub name: String,
    pub protocol: i32,
    pub latency_ms: u128,
}
pub fn status_json(text: &str) -> Result<(String, i32)> {
    let j: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let protocol = j["version"]["protocol"]
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or("Missing/invalid status protocol")?;
    let name = j["version"]["name"]
        .as_str()
        .ok_or("Missing status version name")?;
    if name.len() > 512 {
        return Err("Status version name too long".into());
    }
    Ok((name.into(), protocol))
}
// Standalone diagnostic probe. The in-game probe uses Horizon's SRV resolver.
pub fn probe(address_text: &str, timeout: Duration) -> Result<ServerStatus> {
    if timeout.is_zero() {
        return Err("Timeout must be positive".into());
    }
    let (host, port) = address(address_text)?;
    let addrs: Vec<_> = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .take(16)
        .collect();
    let deadline = Instant::now() + timeout;
    let mut stream = None;
    for addr in addrs {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        if let Ok(s) = TcpStream::connect_timeout(&addr, left.min(Duration::from_secs(1))) {
            stream = Some(s);
            break;
        }
    }
    let mut stream = stream.ok_or("Could not connect to server before timeout")?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    stream.set_nodelay(true).map_err(|e| e.to_string())?;
    write_frame(
        &mut stream,
        &Handshake {
            protocol: CLIENT_PROTOCOL,
            host,
            port,
            intent: 1,
        }
        .body(),
    )?;
    write_frame(&mut stream, &[0])?;
    let response = read_frame(&mut stream)?;
    let mut r = Cursor::new(&response);
    if r.varint()? != 0 {
        return Err("Unexpected status response".into());
    }
    let (name, protocol) = status_json(&r.string(32767)?)?;
    r.end()?;
    let mut ping = vec![1];
    ping.extend_from_slice(&1i64.to_be_bytes());
    let start = Instant::now();
    write_frame(&mut stream, &ping)?;
    if read_frame(&mut stream)? != ping {
        return Err("Mismatched status pong".into());
    }
    Ok(ServerStatus {
        name,
        protocol,
        latency_ms: start.elapsed().as_millis(),
    })
}
