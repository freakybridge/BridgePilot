use super::*;

fn put16(body: &mut [u8], at: usize, value: u16) { body[at..at + 2].copy_from_slice(&value.to_le_bytes()); }
fn put32(body: &mut [u8], at: usize, value: u32) { body[at..at + 4].copy_from_slice(&value.to_le_bytes()); }

fn image() -> Vec<u8> {
    let mut body = vec![0; 0x500];
    body[..2].copy_from_slice(b"MZ");
    put32(&mut body, 0x3c, 0x80);
    body[0x80..0x84].copy_from_slice(b"PE\0\0");
    put16(&mut body, 0x86, 2);
    put16(&mut body, 0x94, 240);
    put16(&mut body, 0x98, 0x20b);
    put32(&mut body, 0x104, 16);
    put32(&mut body, 0x138, 0x2000);
    put32(&mut body, 0x13c, 28);
    for (section, rva, raw, size, flags) in [(0x188, 0x1000, 0x200, 0x100, 0x60000020), (0x1b0, 0x2000, 0x300, 0x200, 0x40000040)] {
        put32(&mut body, section + 12, rva);
        put32(&mut body, section + 16, size);
        put32(&mut body, section + 20, raw);
        put32(&mut body, section + 36, flags);
    }
    body[0x200] = 0xc3;
    put32(&mut body, 0x30c, 2);
    put32(&mut body, 0x310, 40);
    put32(&mut body, 0x314, 0x2040);
    put32(&mut body, 0x318, 0x340);
    body[0x340..0x344].copy_from_slice(b"RSDS");
    body[0x358..0x35f].copy_from_slice(b"bp.pdb\0");
    body
}

#[test]
fn rebuild_metadata_can_differ_but_code_data_and_debug_pairing_structure_cannot() {
    let original = image();
    let mut rebuilt = original.clone();
    put32(&mut rebuilt, 0x88, 123);
    put32(&mut rebuilt, 0x304, 123);
    rebuilt[0x344..0x354].fill(42);
    assert!(equivalent(&original, &rebuilt));
    for at in [0x200, 0x280, 0x390, 0x354, 0x358, 0x98, 0x310, 0x318, 0x1d4] {
        let mut changed = rebuilt.clone();
        changed[at] ^= 1;
        assert!(!equivalent(&original, &changed), "unexpectedly ignored {at:x}");
    }
    rebuilt.push(0);
    assert!(!equivalent(&original, &rebuilt));
}

#[test]
fn malformed_and_executable_debug_ranges_are_never_ignored() {
    for (at, value) in [(0x3c, u32::MAX), (0x138, 0x1000), (0x13c, 27), (0x13c, u32::MAX),
        (0x1c4, 0x200), (0x1d4, 0x60000040), (0x314, 0x2041), (0x318, 0x301), (0x310, u32::MAX)] {
        let mut left = image();
        put32(&mut left, at, value);
        let mut right = left.clone();
        right[0x88] = 7;
        assert!(!equivalent(&left, &right), "accepted malformed {at:x}");
    }
    for length in 0..image().len() {
        let left = image()[..length].to_vec();
        let mut right = left.clone();
        if length > 0x88 { right[0x88] = 8; assert!(!equivalent(&left, &right)); }
    }
}

#[test]
fn exact_non_pe_files_match_but_different_non_pe_files_never_do() {
    assert!(equivalent(b"same", b"same"));
    assert!(!equivalent(b"same", b"diff"));
}
