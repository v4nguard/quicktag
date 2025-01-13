use std::io::SeekFrom;

use binrw::BinRead;

use crate::gui::dxgi::{DxgiFormat, XenosSurfaceFormat};

#[derive(Debug, BinRead)]
pub struct TextureHeaderDevAlphaX360 {
    #[br(seek_before = SeekFrom::Start(0x20))]
    _dataformat: u32,

    #[br(calc((_dataformat & 0x80) != 0))]
    pub unk_flag: bool,

    #[br(try_calc(XenosSurfaceFormat::try_from(_dataformat as u8 & 0x3F)))]
    pub format: XenosSurfaceFormat,

    #[br(seek_before = SeekFrom::Start(0x36))]
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    // pub flags1: u32,
    // pub flags2: u32,
    // pub flags3: u32,
    #[br(seek_before = SeekFrom::Start(0x48), assert(beefcafe == 0xbeefcafe))]
    pub beefcafe: u32,
}

#[derive(Debug, BinRead)]
pub struct TextureHeaderRoiXbox {
    pub format: DxgiFormat,

    #[br(seek_before = SeekFrom::Start(0x2c), assert(beefcafe == 0xbeefcafe))]
    pub beefcafe: u32,

    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub array_size: u16,
    // pub flags1: u32,
    // pub flags2: u32,
    // pub flags3: u32,
}
