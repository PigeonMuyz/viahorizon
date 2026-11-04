// SPDX-License-Identifier: GPL-3.0-or-later
// Java HashOps / CRC32C value hashing. Integers are little-endian, strings use
// Java UTF-16 code units, and maps sort unsigned (key hash, value hash) pairs.
const fn table() -> [u32; 256] {
    let mut out = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x82f63b78
            } else {
                crc >> 1
            };
            bit += 1;
        }
        out[i] = crc;
        i += 1;
    }
    out
}
const TABLE: [u32; 256] = table();
struct Crc(u32);
impl Crc {
    fn new() -> Self {
        Self(!0)
    }
    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = TABLE[((self.0 ^ u32::from(b)) & 255) as usize] ^ (self.0 >> 8);
        }
    }
    fn finish(self) -> i32 {
        (!self.0) as i32
    }
}
pub fn crc32c(bytes: &[u8]) -> i32 {
    let mut crc = Crc::new();
    crc.bytes(bytes);
    crc.finish()
}
fn scalar(kind: u8, bytes: &[u8]) -> i32 {
    let mut crc = Crc::new();
    crc.bytes(&[kind]);
    crc.bytes(bytes);
    crc.finish()
}
pub fn byte(v: i8) -> i32 {
    scalar(6, &[v as u8])
}
pub fn short(v: i16) -> i32 {
    scalar(7, &v.to_le_bytes())
}
pub fn int(v: i32) -> i32 {
    scalar(8, &v.to_le_bytes())
}
pub fn long(v: i64) -> i32 {
    scalar(9, &v.to_le_bytes())
}
pub fn float(v: f32) -> i32 {
    scalar(
        10,
        &if v.is_nan() {
            0x7fc00000u32
        } else {
            v.to_bits()
        }
        .to_le_bytes(),
    )
}
pub fn double(v: f64) -> i32 {
    scalar(
        11,
        &if v.is_nan() {
            0x7ff8000000000000u64
        } else {
            v.to_bits()
        }
        .to_le_bytes(),
    )
}
pub fn boolean(v: bool) -> i32 {
    scalar(13, &[u8::from(v)])
}
pub fn utf16(units: &[u16]) -> i32 {
    let mut crc = Crc::new();
    crc.bytes(&[12]);
    crc.bytes(&(units.len() as u32).to_le_bytes());
    for unit in units {
        crc.bytes(&unit.to_le_bytes());
    }
    crc.finish()
}
pub fn string(value: &str) -> i32 {
    utf16(&value.encode_utf16().collect::<Vec<_>>())
}
pub fn bytes(values: &[u8]) -> i32 {
    let mut crc = Crc::new();
    crc.bytes(&[14]);
    crc.bytes(values);
    crc.bytes(&[15]);
    crc.finish()
}
pub fn ints(values: &[i32]) -> i32 {
    let mut crc = Crc::new();
    crc.bytes(&[16]);
    for v in values {
        crc.bytes(&v.to_le_bytes());
    }
    crc.bytes(&[17]);
    crc.finish()
}
pub fn longs(values: &[i64]) -> i32 {
    let mut crc = Crc::new();
    crc.bytes(&[18]);
    for v in values {
        crc.bytes(&v.to_le_bytes());
    }
    crc.bytes(&[19]);
    crc.finish()
}
pub fn list(values: impl IntoIterator<Item = i32>) -> i32 {
    let mut crc = Crc::new();
    crc.bytes(&[4]);
    for v in values {
        crc.bytes(&v.to_le_bytes());
    }
    crc.bytes(&[5]);
    crc.finish()
}
pub fn map(values: impl IntoIterator<Item = (i32, i32)>) -> i32 {
    let mut values = values
        .into_iter()
        .map(|(k, v)| (k as u32, v as u32))
        .collect::<Vec<_>>();
    values.sort_unstable();
    let mut crc = Crc::new();
    crc.bytes(&[2]);
    for (k, v) in values {
        crc.bytes(&k.to_le_bytes());
        crc.bytes(&v.to_le_bytes());
    }
    crc.bytes(&[3]);
    crc.finish()
}
