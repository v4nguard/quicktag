use std::fmt::Debug;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct XgSampleDesc {
    pub count: u32,
    pub quality: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct XgTexture2DDesc {
    pub width: u32,
    pub height: u32,
    pub mip_levels: u32,
    pub array_size: u32,
    pub format: u32,
    pub sample_desc: XgSampleDesc,
    pub usage: u32,
    pub bind_flags: u32,
    pub cpu_access_flags: u32,
    pub misc_flags: u32,
    pub esram_offset_bytes: u32,
    pub esram_usage_bytes: u32,
    pub tile_mode: u32,
    pub pitch: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct XgMipMap {
    pub size_bytes: u64,                     // 0x8
    pub offset_bytes: u64,                   // 0x10
    pub slice_2d_size_bytes: u64,            // 0x18
    pub pitch_pixels: u32,                   // 0x1C
    pub pitch_bytes: u32,                    // 0x20
    pub alignment_bytes: u32,                // 0x24
    pub padded_width_elements: u32,          // 0x28
    pub padded_height_elements: u32,         // 0x2C
    pub padded_depth_or_array_size: u32,     // 0x30
    pub width_elements: u32,                 // 0x34
    pub height_elements: u32,                // 0x38
    pub depth_or_array_size: u32,            // 0x3C
    pub sample_count: u32,                   // 0x40
    pub tile_mode: u32,                      // 0x44
    pub padding1: i32,                       // 0x48
    pub bank_rotation_address_bit_mask: u64, // 0x50
    pub bank_rotation_bytes_per_slice: u64,  // 0x58
    pub slice_depth_elements: u32,           // 0x5C
    pub padding2: i32,                       // 0x60
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct XgPlaneLayout {
    pub usage: u32,                // 0x4
    pub padding1: i32,             // 0x8
    pub size_bytes: u64,           // 0x10
    pub base_offset_bytes: u64,    // 0x18
    pub base_alignment_bytes: u64, // 0x20
    pub bytes_per_element: u32,    // 0x24
    pub padding2: i32,             // 0x28

    pub mips: [XgMipMap; 15], // 0x5A0
}

#[repr(C)]
#[derive(Clone)]
pub struct XgResourceLayout {
    pub size_bytes: u64,
    pub base_alignment_bytes: u64,
    pub mip_levels: u32,

    pub plane_count: u32,
    pub planes: [XgPlaneLayout; 4],

    pub dimension: u32,
    pub padding: u32,
}

impl Debug for XgResourceLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XgResourceLayout")
            .field("size_bytes", &self.size_bytes)
            .field("base_alignment_bytes", &self.base_alignment_bytes)
            .field("mip_levels", &self.mip_levels)
            .field("plane_count", &self.plane_count)
            .field("planes", &&self.planes[..self.plane_count as usize]) // Only print used planes
            .field("dimension", &self.dimension)
            .finish()
    }
}
