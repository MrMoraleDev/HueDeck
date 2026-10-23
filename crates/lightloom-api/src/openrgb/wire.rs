// SPDX-License-Identifier: GPL-3.0-or-later
//! The OpenRGB SDK wire format, written from the protocol facts collected in
//! `docs/research/m6-windows-integrations.md` §2 (no OpenRGB code or text):
//! a 16-byte header (`"ORGB"`, `u32` device index, `u32` packet id, `u32`
//! payload size), little-endian integers, strings as `u16` length (counting
//! the NUL) + bytes + NUL, colours as `R, G, B, 0`. Size-prefixed blocks
//! (`data_size`) count their own four bytes.

/// Header magic.
pub const MAGIC: [u8; 4] = *b"ORGB";
/// Header length.
pub const HEADER_LEN: usize = 16;
/// Largest payload accepted from a client (a full 65535-LED update is ~256 KiB).
pub const MAX_PAYLOAD: u32 = 1 << 20;
/// Highest protocol version this server speaks.
pub const SERVER_PROTOCOL: u32 = 4;

/// Packet ids used by the server.
pub mod pid {
    /// C→S: how many controllers; reply `u32`.
    pub const REQUEST_CONTROLLER_COUNT: u32 = 0;
    /// C→S: one controller's data; reply `data_size` + Device Data.
    pub const REQUEST_CONTROLLER_DATA: u32 = 1;
    /// C→S: protocol negotiation (v1+); reply `u32` server maximum.
    pub const REQUEST_PROTOCOL_VERSION: u32 = 40;
    /// C→S: NUL-terminated client name.
    pub const SET_CLIENT_NAME: u32 = 50;
    /// S→C: the controller list changed (v1+).
    pub const DEVICE_LIST_UPDATED: u32 = 100;
    /// C→S: rescan devices (ignored).
    pub const REQUEST_RESCAN_DEVICES: u32 = 140;
    /// C→S: profile list (v2+); answered with an empty list.
    pub const REQUEST_PROFILE_LIST: u32 = 150;
    /// C→S: save a profile (unsupported, ignored).
    pub const REQUEST_SAVE_PROFILE: u32 = 151;
    /// C→S: load a profile (unsupported, ignored).
    pub const REQUEST_LOAD_PROFILE: u32 = 152;
    /// C→S: delete a profile (unsupported, ignored).
    pub const REQUEST_DELETE_PROFILE: u32 = 153;
    /// C→S: plugin list (v4+); answered with an empty list.
    pub const REQUEST_PLUGIN_LIST: u32 = 200;
    /// C→S: plugin-specific packet (ignored).
    pub const PLUGIN_SPECIFIC: u32 = 201;
    /// C→S: `i32 zone, i32 new_size`.
    pub const RESIZEZONE: u32 = 1000;
    /// C→S: `data_size, u16 n, n colours`.
    pub const UPDATELEDS: u32 = 1050;
    /// C→S: `data_size, u32 zone, u16 n, n colours`.
    pub const UPDATEZONELEDS: u32 = 1051;
    /// C→S: `i32 led, colour`.
    pub const UPDATESINGLELED: u32 = 1052;
    /// C→S: switch to the "Direct"-like mode.
    pub const SETCUSTOMMODE: u32 = 1100;
    /// C→S: `data_size, i32 mode, Mode`.
    pub const UPDATEMODE: u32 = 1101;
    /// C→S: like UPDATEMODE, also saved to the device (v3+).
    pub const SAVEMODE: u32 = 1102;
}

/// Mode flag: the mode takes per-LED colours.
pub const MODE_FLAG_HAS_PER_LED_COLOR: u32 = 1 << 5;
/// Mode colour mode: per-LED.
pub const MODE_COLORS_PER_LED: u32 = 1;
/// Zone type: one LED.
pub const ZONE_TYPE_SINGLE: i32 = 0;
/// Zone type: a line of LEDs.
pub const ZONE_TYPE_LINEAR: i32 = 1;

/// A packet header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Controller index (`pkt_dev_idx`).
    pub dev: u32,
    /// Packet id.
    pub id: u32,
    /// Payload bytes after the header.
    pub len: u32,
}

/// A header that cannot be accepted: the connection is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderError {
    /// Not `"ORGB"`.
    Magic,
    /// Payload larger than [`MAX_PAYLOAD`].
    TooLarge(u32),
}

impl std::fmt::Display for HeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeaderError::Magic => write!(f, "not an OpenRGB SDK packet (bad magic)"),
            HeaderError::TooLarge(n) => write!(f, "payload of {n} bytes is larger than {MAX_PAYLOAD}"),
        }
    }
}

impl Header {
    /// Parse and check a header.
    pub fn decode(b: &[u8; HEADER_LEN]) -> Result<Self, HeaderError> {
        if b[..4] != MAGIC {
            return Err(HeaderError::Magic);
        }
        let u = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let h = Header {
            dev: u(4),
            id: u(8),
            len: u(12),
        };
        if h.len > MAX_PAYLOAD {
            return Err(HeaderError::TooLarge(h.len));
        }
        Ok(h)
    }
}

/// A whole packet: header followed by `payload`.
pub fn packet(dev: u32, id: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&dev.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// Little-endian writer.
#[derive(Default)]
struct Out(Vec<u8>);

impl Out {
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    /// `u16` length including the NUL, the bytes, the NUL (cut to fit).
    fn str(&mut self, s: &str) {
        let mut end = s.len().min(u16::MAX as usize - 1);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        let bytes: Vec<u8> = s.as_bytes()[..end]
            .iter()
            .map(|&b| if b == 0 { b' ' } else { b })
            .collect();
        self.u16(bytes.len() as u16 + 1);
        self.0.extend_from_slice(&bytes);
        self.0.push(0);
    }
    fn color(&mut self, c: [u8; 3]) {
        self.0.extend_from_slice(&[c[0], c[1], c[2], 0]);
    }
    fn count(&mut self, n: usize) -> usize {
        let n = n.min(u16::MAX as usize);
        self.u16(n as u16);
        n
    }
}

/// A mode of a controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mode {
    /// Name ("Direct").
    pub name: String,
    /// Device-specific value (sent for every version up to 5).
    pub value: i32,
    /// `MODE_FLAG_*` bits.
    pub flags: u32,
    /// Speed range and value.
    pub speed_min: u32,
    /// Speed maximum.
    pub speed_max: u32,
    /// Brightness range and value (v3+).
    pub brightness_min: u32,
    /// Brightness maximum.
    pub brightness_max: u32,
    /// Mode-specific colour count range.
    pub colors_min: u32,
    /// Colour count maximum.
    pub colors_max: u32,
    /// Speed.
    pub speed: u32,
    /// Brightness (v3+).
    pub brightness: u32,
    /// Direction.
    pub direction: u32,
    /// Colour mode (`MODE_COLORS_*`).
    pub color_mode: u32,
    /// Mode-specific colours.
    pub colors: Vec<[u8; 3]>,
}

impl Mode {
    /// The per-LED "Direct" mode, the only mode Lightloom controllers have.
    pub fn direct() -> Self {
        Self {
            name: "Direct".into(),
            value: 0,
            flags: MODE_FLAG_HAS_PER_LED_COLOR,
            speed_min: 0,
            speed_max: 0,
            brightness_min: 0,
            brightness_max: 0,
            colors_min: 0,
            colors_max: 0,
            speed: 0,
            brightness: 0,
            direction: 0,
            color_mode: MODE_COLORS_PER_LED,
            colors: Vec::new(),
        }
    }

    fn encode(&self, o: &mut Out, v: u32) {
        o.str(&self.name);
        o.i32(self.value);
        o.u32(self.flags);
        o.u32(self.speed_min);
        o.u32(self.speed_max);
        if v >= 3 {
            o.u32(self.brightness_min);
            o.u32(self.brightness_max);
        }
        o.u32(self.colors_min);
        o.u32(self.colors_max);
        o.u32(self.speed);
        if v >= 3 {
            o.u32(self.brightness);
        }
        o.u32(self.direction);
        o.u32(self.color_mode);
        let n = o.count(self.colors.len());
        for c in &self.colors[..n] {
            o.color(*c);
        }
    }
}

/// A zone (Lightloom: one canvas endpoint).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zone {
    /// Name.
    pub name: String,
    /// `ZONE_TYPE_*`.
    pub ty: i32,
    /// Smallest size.
    pub leds_min: u32,
    /// Largest size.
    pub leds_max: u32,
    /// LEDs now.
    pub leds_count: u32,
}

impl Zone {
    fn encode(&self, o: &mut Out, v: u32) {
        o.str(&self.name);
        o.i32(self.ty);
        o.u32(self.leds_min);
        o.u32(self.leds_max);
        o.u32(self.leds_count);
        // No matrix map.
        o.u16(0);
        if v >= 4 {
            // No segments.
            o.u16(0);
        }
    }
}

/// What REQUEST_CONTROLLER_DATA describes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Controller {
    /// Device type (OpenRGB numbering).
    pub ty: i32,
    /// Name.
    pub name: String,
    /// Vendor (v1+).
    pub vendor: String,
    /// Description.
    pub description: String,
    /// Version.
    pub version: String,
    /// Serial.
    pub serial: String,
    /// Location.
    pub location: String,
    /// Modes.
    pub modes: Vec<Mode>,
    /// Index of the active mode.
    pub active_mode: i32,
    /// Zones.
    pub zones: Vec<Zone>,
    /// LED names, zone after zone.
    pub leds: Vec<String>,
    /// Current colours, one per LED.
    pub colors: Vec<[u8; 3]>,
}

impl Controller {
    /// Reply payload of REQUEST_CONTROLLER_DATA at protocol version `v`:
    /// `u32 data_size` (counting itself) followed by the Device Data.
    pub fn encode(&self, v: u32) -> Vec<u8> {
        let mut o = Out::default();
        o.u32(0);
        o.i32(self.ty);
        o.str(&self.name);
        if v >= 1 {
            o.str(&self.vendor);
        }
        o.str(&self.description);
        o.str(&self.version);
        o.str(&self.serial);
        o.str(&self.location);
        let n = o.count(self.modes.len());
        o.i32(self.active_mode);
        for m in &self.modes[..n] {
            m.encode(&mut o, v);
        }
        let n = o.count(self.zones.len());
        for z in &self.zones[..n] {
            z.encode(&mut o, v);
        }
        let n = o.count(self.leds.len());
        for name in &self.leds[..n] {
            o.str(name);
            // led_value
            o.u32(0);
        }
        let n = o.count(self.colors.len());
        for c in &self.colors[..n] {
            o.color(*c);
        }
        let size = o.0.len() as u32;
        o.0[..4].copy_from_slice(&size.to_le_bytes());
        o.0
    }
}

/// `u32` payload (counts, versions).
pub fn u32_payload(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// An empty profile or plugin list: `data_size` 6, `u16` 0.
pub fn empty_list() -> Vec<u8> {
    let mut o = Out::default();
    o.u32(6);
    o.u16(0);
    o.0
}

/// A payload that does not have the documented layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Malformed;

/// Little-endian reader over a payload.
pub struct In<'a>(&'a [u8]);

impl<'a> In<'a> {
    /// Read `payload` from the start.
    pub fn new(payload: &'a [u8]) -> Self {
        Self(payload)
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], Malformed> {
        if self.0.len() < n {
            return Err(Malformed);
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    /// `u16`.
    pub fn u16(&mut self) -> Result<u16, Malformed> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    /// `u32`.
    pub fn u32(&mut self) -> Result<u32, Malformed> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    /// `i32`.
    pub fn i32(&mut self) -> Result<i32, Malformed> {
        self.u32().map(|v| v as i32)
    }
    /// One colour (`R, G, B, pad`).
    pub fn color(&mut self) -> Result<[u8; 3], Malformed> {
        let b = self.take(4)?;
        Ok([b[0], b[1], b[2]])
    }
    /// `u16 n` + `n` colours.
    pub fn colors(&mut self) -> Result<Vec<[u8; 3]>, Malformed> {
        let n = self.u16()? as usize;
        (0..n).map(|_| self.color()).collect()
    }
}

/// UPDATELEDS: `data_size, u16 n, n colours`.
pub fn parse_update_leds(p: &[u8]) -> Result<Vec<[u8; 3]>, Malformed> {
    let mut r = In::new(p);
    r.u32()?;
    r.colors()
}

/// UPDATEZONELEDS: `data_size, u32 zone, u16 n, n colours`.
pub fn parse_update_zone_leds(p: &[u8]) -> Result<(u32, Vec<[u8; 3]>), Malformed> {
    let mut r = In::new(p);
    r.u32()?;
    let zone = r.u32()?;
    Ok((zone, r.colors()?))
}

/// UPDATESINGLELED: `i32 led, colour`.
pub fn parse_update_single_led(p: &[u8]) -> Result<(i32, [u8; 3]), Malformed> {
    let mut r = In::new(p);
    Ok((r.i32()?, r.color()?))
}

/// UPDATEMODE / SAVEMODE: `data_size, i32 mode, Mode`; only the index matters.
pub fn parse_mode_index(p: &[u8]) -> Result<i32, Malformed> {
    let mut r = In::new(p);
    r.u32()?;
    r.i32()
}

/// RESIZEZONE: `i32 zone, i32 new_size`.
pub fn parse_resize_zone(p: &[u8]) -> Result<(i32, i32), Malformed> {
    let mut r = In::new(p);
    Ok((r.i32()?, r.i32()?))
}

/// A NUL-terminated string payload (SET_CLIENT_NAME).
pub fn parse_c_string(p: &[u8]) -> String {
    let end = p.iter().position(|&b| b == 0).unwrap_or(p.len());
    String::from_utf8_lossy(&p[..end]).into_owned()
}

/// OpenRGB device type for a Lightloom `DeviceType()` (numbers valid for
/// OpenRGB 0.9+ clients; older clients read larger values as Unknown).
pub fn device_type(lightloom: &str) -> i32 {
    match lightloom {
        "motherboard" => 0,
        "ram" => 1,
        "gpu" => 2,
        "aio" => 3,
        "lightingcontroller" => 4,
        "keyboard" => 5,
        "mouse" => 6,
        "mousepad" => 7,
        "headphones" => 8,
        "speakers" => 12,
        "case" => 15,
        "microphone" => 16,
        // other, dongle, lcd, anything new: Accessory.
        _ => 17,
    }
}
