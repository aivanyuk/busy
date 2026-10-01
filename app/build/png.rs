//! A minimal PNG encoder for the large icon images and the Store's logos: RGBA, each row filtered "Up" (so the
//! glyph's flat areas become runs of zeros), compressed with one fixed-Huffman deflate block that codes runs as
//! distance-1 matches. Enough to keep each image a few kilobytes rather than its raw size (256 KB for the
//! 256-pixel icon).

/// Writes bits least significant first, as deflate does.
struct Bits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Bits {
    fn put(&mut self, v: u32, n: u32) {
        self.acc |= v << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// A Huffman code, which deflate stores most significant bit first.
    fn code(&mut self, v: u32, n: u32) {
        let rev = (0..n).fold(0, |r, i| r | (((v >> i) & 1) << (n - 1 - i)));
        self.put(rev, n);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

/// Fixed-Huffman literal/length code `v` (0..=287).
fn lit(b: &mut Bits, v: u32) {
    match v {
        0..=143 => b.code(0x30 + v, 8),
        144..=255 => b.code(0x190 + v - 144, 9),
        256..=279 => b.code(v - 256, 7),
        _ => b.code(0xC0 + v - 280, 8),
    }
}

const LEN_BASE: [u32; 29] =
    [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut b = Bits { out: Vec::new(), acc: 0, n: 0 };
    b.put(1, 1); // final block
    b.put(1, 2); // fixed Huffman
    let mut i = 0;
    while i < data.len() {
        let run = if i > 0 { data[i..].iter().take(258).take_while(|&&x| x == data[i - 1]).count() } else { 0 };
        if run >= 3 {
            let k = LEN_BASE.iter().rposition(|&l| l <= run as u32).unwrap_or(0);
            lit(&mut b, 257 + k as u32);
            b.put(run as u32 - LEN_BASE[k], LEN_EXTRA[k]);
            b.code(0, 5); // distance 1
            i += run;
        } else {
            lit(&mut b, u32::from(data[i]));
            i += 1;
        }
    }
    lit(&mut b, 256);
    b.finish()
}

fn crc32(data: &[u8]) -> u32 {
    !data.iter().fold(!0u32, |c, &x| {
        (0..8).fold(c ^ u32::from(x), |c, _| if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 })
    })
}

fn adler32(data: &[u8]) -> u32 {
    let (a, b) = data.iter().fold((1u32, 0u32), |(a, b), &x| {
        let a = (a + u32::from(x)) % 65521;
        (a, (b + a) % 65521)
    });
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, ty: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(ty);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// `size`² straight-alpha RGBA pixels, top row first.
pub fn encode(size: u32, px: &[[u8; 4]]) -> Vec<u8> {
    let stride = size as usize * 4;
    let flat: Vec<u8> = px.iter().flatten().copied().collect();
    let mut raw = Vec::with_capacity(flat.len() + size as usize);
    for (y, row) in flat.chunks(stride).enumerate() {
        raw.push(2); // "Up": each byte minus the one above
        for (x, &v) in row.iter().enumerate() {
            raw.push(if y == 0 { v } else { v.wrapping_sub(flat[(y - 1) * stride + x]) });
        }
    }
    let mut z = vec![0x78, 0x01];
    z.extend_from_slice(&deflate(&raw));
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}
