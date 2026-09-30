//! A compiled resource (`.res`) file: the format `rc.exe` writes and link.exe reads. Holds the version
//! information (`RT_VERSION` 1) and the icon (`RT_GROUP_ICON` 1, its images `RT_ICON` 1..).

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;
/// English (US), the language of the strings below.
const LANG: u16 = 0x0409;
/// MOVEABLE | PURE, as rc.exe marks these resources.
const FLAGS: u16 = 0x0030;

/// `1.2.3` → `1.2.3.0` (Windows versions have four parts; pre-release suffixes are dropped).
pub fn four_part(v: &str) -> String {
    let mut p: Vec<u16> = v.split(['.', '-', '+']).take(3).map(|s| s.parse().unwrap_or(0)).collect();
    p.resize(4, 0);
    p.iter().map(u16::to_string).collect::<Vec<_>>().join(".")
}

fn pad4(b: &mut Vec<u8>) {
    while !b.len().is_multiple_of(4) {
        b.push(0);
    }
}

fn u16s(b: &mut Vec<u8>, v: &[u16]) {
    v.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
}

fn utf16z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// One resource: header (type and name by ordinal) and data, each padded to 4 bytes.
fn resource(out: &mut Vec<u8>, ty: u16, id: u16, data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes()); // header size
    u16s(out, &[0xFFFF, ty, 0xFFFF, id]);
    out.extend_from_slice(&0u32.to_le_bytes()); // data version
    u16s(out, &[FLAGS, LANG]);
    out.extend_from_slice(&[0; 8]); // version, characteristics
    out.extend_from_slice(data);
    pad4(out);
}

/// A `VS_VERSIONINFO` node: length, value length, type, key, value, children; each part 4-aligned.
fn node(key: &str, value: &[u8], value_len: u16, text: bool, children: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![0, 0];
    u16s(&mut b, &[value_len, u16::from(text)]);
    u16s(&mut b, &utf16z(key));
    pad4(&mut b);
    b.extend_from_slice(value);
    for c in children {
        pad4(&mut b);
        b.extend_from_slice(c);
    }
    let len = b.len() as u16;
    b[..2].copy_from_slice(&len.to_le_bytes());
    b
}

fn string(key: &str, value: &str) -> Vec<u8> {
    let v = utf16z(value);
    let mut bytes = Vec::new();
    u16s(&mut bytes, &v);
    node(key, &bytes, v.len() as u16, true, &[])
}

fn version_info(version: &str, copyright: &str) -> Vec<u8> {
    let parts: Vec<u32> = four_part(version).split('.').map(|s| s.parse().unwrap_or(0)).collect();
    let (ms, ls) = ((parts[0] << 16) | parts[1], (parts[2] << 16) | parts[3]);
    let mut fixed = Vec::new();
    for v in [0xFEEF04BD, 0x0001_0000, ms, ls, ms, ls, 0x3F, 0, 0x0004_0004, 1, 0, 0, 0] {
        fixed.extend_from_slice(&u32::to_le_bytes(v));
    }
    let strings = [
        ("FileDescription", "busy - taskbar system monitor"),
        ("FileVersion", version),
        ("InternalName", "busy"),
        ("LegalCopyright", copyright),
        ("OriginalFilename", "busy.exe"),
        ("ProductName", "busy"),
        ("ProductVersion", version),
    ];
    let table = node("040904B0", &[], 0, true, &strings.map(|(k, v)| string(k, v)));
    let string_info = node("StringFileInfo", &[], 0, true, &[table]);
    let mut translation = Vec::new();
    u16s(&mut translation, &[LANG, 1200]);
    let var = node("Translation", &translation, 4, false, &[]);
    let var_info = node("VarFileInfo", &[], 0, true, &[var]);
    node("VS_VERSION_INFO", &fixed, fixed.len() as u16, false, &[string_info, var_info])
}

/// The whole `.res` file for `version`, the `copyright` line and the icon's `images` (size, `RT_ICON` data).
pub fn file(version: &str, copyright: &str, images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    // A .res file starts with an empty resource that marks it as 32-bit.
    let mut out = Vec::new();
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    u16s(&mut out, &[0xFFFF, 0, 0xFFFF, 0]);
    out.extend_from_slice(&[0; 16]);
    let mut group = Vec::new();
    u16s(&mut group, &[0, 1, images.len() as u16]);
    for (i, (size, data)) in images.iter().enumerate() {
        let id = i as u16 + 1;
        resource(&mut out, RT_ICON, id, data);
        // 256 is written as 0.
        let side = if *size >= 256 { 0 } else { *size as u8 };
        group.extend_from_slice(&[side, side, 0, 0]);
        u16s(&mut group, &[1, 32]);
        group.extend_from_slice(&(data.len() as u32).to_le_bytes());
        u16s(&mut group, &[id]);
    }
    resource(&mut out, RT_GROUP_ICON, 1, &group);
    resource(&mut out, RT_VERSION, 1, &version_info(version, copyright));
    out
}
