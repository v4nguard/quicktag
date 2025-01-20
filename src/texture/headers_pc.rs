use binrw::BinRead;
use destiny_pkg::TagHash;

use super::dxgi::DxgiFormat;

#[derive(Debug, BinRead)]
#[br(import(prebl: bool))]
pub struct TextureHeaderPC {
    pub data_size: u32,
    pub format: DxgiFormat,
    pub _unk8: u32,

    #[br(if(!prebl))]
    pub _unkc: [u32; 5],

    #[br(assert(cafe == 0xcafe))]
    pub cafe: u16, // prebl: 0xc / bl: 0x20

    pub width: u16,      // prebl: 0xe / bl: 0x22
    pub height: u16,     // prebl: 0x10 / bl: 0x24
    pub depth: u16,      // prebl: 0x12 / bl: 0x26
    pub array_size: u16, // prebl: 0x14 / bl: 0x28

    pub _pad0: [u16; 7], // prebl: 0x16 / bl: 0x2a

    #[br(if(!prebl))]
    pub _pad1: u32,

    // pub _unk2a: [u32; 4]
    // pub unk2a: u16,
    // pub unk2c: u8,
    // pub mip_count: u8,
    // pub unk2e: [u8; 10],
    // pub unk38: u32,
    #[br(map(|v: u32| (v != u32::MAX).then_some(TagHash(v))))]
    pub large_buffer: Option<TagHash>, // prebl: 0x24 / bl: 0x3c
}
