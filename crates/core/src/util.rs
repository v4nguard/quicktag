use tiger_pkg::Endian;

pub const FNV1_BASE: u32 = 0x811c9dc5;
pub const FNV1_PRIME: u32 = 0x01000193;
pub fn fnv1(data: &[u8]) -> u32 {
    data.iter().fold(FNV1_BASE, |acc, b| {
        acc.wrapping_mul(FNV1_PRIME) ^ (*b as u32)
    })
}

#[inline(always)]
pub fn u64_from_endian(endian: Endian, bytes: [u8; 8]) -> u64 {
    match endian {
        Endian::Big => u64::from_be_bytes(bytes),
        Endian::Little => u64::from_le_bytes(bytes),
    }
}

#[inline(always)]
pub fn u32_from_endian(endian: Endian, bytes: [u8; 4]) -> u32 {
    match endian {
        Endian::Big => u32::from_be_bytes(bytes),
        Endian::Little => u32::from_le_bytes(bytes),
    }
}
