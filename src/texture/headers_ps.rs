use std::io::SeekFrom;

use binrw::BinRead;
use destiny_pkg::TagHash;

use crate::gui::dxgi::{GcmSurfaceFormat, GcnSurfaceFormat};

#[derive(Debug, BinRead)]
#[br(import(prebl: bool))]
pub struct TextureHeaderD2Ps4 {
    pub data_size: u32,
    pub unk4: u8,
    pub unk5: u8,
    #[br(try_map(|v: u16| GcnSurfaceFormat::try_from((v >> 4) & 0x3F)))]
    pub format: GcnSurfaceFormat,

    #[br(if(prebl))]
    pub _unkc: [u32; 8],

    #[br(assert(cafe == 0xcafe))]
    pub cafe: u16,

    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub array_size: u16,

    pub flags1: u32,
    pub flags2: u32,
    pub flags3: u32,

    #[br(if(prebl))]
    pub _pad1: u16,

    #[br(map(|v: u32| (v != u32::MAX).then_some(TagHash(v))))]
    pub large_buffer: Option<TagHash>, // prebl: 0x24 / bl: 0x3c
}

#[derive(Debug, BinRead)]
pub struct TextureHeaderRoiPs4 {
    pub data_size: u32,
    pub unk4: u8,
    pub unk5: u8,
    #[br(try_map(|v: u16| GcnSurfaceFormat::try_from((v >> 4) & 0x3F)))]
    pub format: GcnSurfaceFormat,

    #[br(seek_before = SeekFrom::Start(0x24), assert(beefcafe == 0xbeefcafe))]
    pub beefcafe: u32,

    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub array_size: u16,

    pub flags1: u32,
    pub flags2: u32,
    pub flags3: u32,
}

#[derive(Debug, BinRead)]
#[br(big)]
pub struct TextureHeaderPs3 {
    // _format: u8,
    // #[br(try_calc(GcmSurfaceFormat::try_from(_format & 0x9F)))] // 0x20: swizzle
    #[br(try_map(|v: u8| GcmSurfaceFormat::try_from(v & 0x9F)))]
    pub format: GcmSurfaceFormat,

    #[br(seek_before = SeekFrom::Start(0x10))]
    pub flags1: u32,

    #[br(seek_before = SeekFrom::Start(0x1C), assert(beefcafe == 0xbeefcafe))]
    pub beefcafe: u32,

    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub array_size: u16,
    // pub flags1: u32,
    // pub flags2: u32,
    // pub flags3: u32,
}
