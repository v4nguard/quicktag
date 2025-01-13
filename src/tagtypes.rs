use std::fmt::Display;

use destiny_pkg::GameVersion;
use eframe::epaint::Color32;

use crate::package_manager::package_manager;

#[derive(PartialEq, Copy, Clone)]
pub enum TagType {
    TextureOld,
    Texture2D { is_header: bool },
    TextureCube { is_header: bool },
    Texture3D { is_header: bool },
    TextureSampler { is_header: bool },
    TextureLargeBuffer,

    VertexBuffer { is_header: bool },
    IndexBuffer { is_header: bool },
    ConstantBuffer { is_header: bool },
    PixelShader { is_header: bool },
    VertexShader { is_header: bool },
    GeometryShader { is_header: bool },
    ComputeShader { is_header: bool },

    WwiseBank,
    WwiseStream,

    Havok,
    OtfFontOrUmbraTome,
    CriwareUsm,

    Tag,
    TagGlobal,

    Unknown { ftype: u8, fsubtype: u8 },
}

impl TagType {
    pub fn is_texture(&self) -> bool {
        matches!(
            self,
            TagType::TextureOld
                | TagType::Texture2D { .. }
                | TagType::TextureCube { .. }
                | TagType::Texture3D { .. }
        )
    }

    pub fn is_header(&self) -> bool {
        matches!(
            self,
            TagType::Texture2D { is_header: true }
                | TagType::TextureCube { is_header: true }
                | TagType::Texture3D { is_header: true }
                | TagType::TextureSampler { is_header: true }
                | TagType::VertexBuffer { is_header: true }
                | TagType::IndexBuffer { is_header: true }
                | TagType::ConstantBuffer { is_header: true }
                | TagType::PixelShader { is_header: true }
                | TagType::VertexShader { is_header: true }
                | TagType::ComputeShader { is_header: true }
        )
    }

    pub fn is_tag(&self) -> bool {
        matches!(self, TagType::Tag | TagType::TagGlobal)
    }

    pub fn is_wwise(&self) -> bool {
        matches!(self, TagType::WwiseBank | TagType::WwiseStream)
    }

    pub fn display_color(&self) -> Color32 {
        match self {
            TagType::TextureOld
            | TagType::Texture2D { .. }
            | TagType::TextureCube { .. }
            | TagType::Texture3D { .. }
            | TagType::TextureSampler { .. }
            | TagType::TextureLargeBuffer { .. } => Color32::GREEN,

            TagType::VertexBuffer { .. }
            | TagType::IndexBuffer { .. }
            | TagType::ConstantBuffer { .. } => Color32::LIGHT_BLUE,

            TagType::PixelShader { .. }
            | TagType::VertexShader { .. }
            | TagType::GeometryShader { .. }
            | TagType::ComputeShader { .. } => Color32::from_rgb(249, 168, 71),

            TagType::WwiseBank | TagType::WwiseStream => Color32::from_rgb(191, 106, 247),
            TagType::Havok | TagType::OtfFontOrUmbraTome | TagType::CriwareUsm => Color32::YELLOW,

            TagType::TagGlobal => Color32::WHITE,
            TagType::Tag => Color32::GRAY,

            TagType::Unknown { .. } => Color32::LIGHT_RED,
        }
    }

    pub fn all_filterable() -> &'static [Self] {
        &[
            Self::Texture2D { is_header: true },
            Self::TextureCube { is_header: true },
            Self::Texture3D { is_header: true },
            Self::TextureSampler { is_header: true },
            Self::TextureLargeBuffer,
            Self::VertexBuffer { is_header: true },
            Self::IndexBuffer { is_header: true },
            Self::ConstantBuffer { is_header: true },
            Self::PixelShader { is_header: true },
            Self::VertexShader { is_header: true },
            Self::GeometryShader { is_header: true },
            Self::ComputeShader { is_header: true },
            Self::WwiseBank,
            Self::WwiseStream,
            Self::Havok,
            Self::OtfFontOrUmbraTome,
            Self::CriwareUsm,
            Self::Tag,
            Self::TagGlobal,
        ]
    }
}

impl Display for TagType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagType::TextureOld => f.write_str("Texture (D1)"),
            TagType::Texture2D { is_header } => f.write_fmt(format_args!(
                "Texture2D{}",
                if *is_header { "" } else { " (Data)" }
            )),
            TagType::TextureCube { is_header } => f.write_fmt(format_args!(
                "TextureCube{}",
                if *is_header { "" } else { " (Data)" }
            )),
            TagType::Texture3D { is_header } => f.write_fmt(format_args!(
                "Texture3D{}",
                if *is_header { "" } else { " (Data)" }
            )),
            TagType::TextureLargeBuffer => f.write_str("TextureLargeBuffer"),
            TagType::TextureSampler { is_header } => f.write_fmt(format_args!(
                "TextureSampler{}",
                if *is_header { "" } else { " (Data)" }
            )),
            TagType::VertexBuffer { is_header } => f.write_fmt(format_args!(
                "VertexBuffer{}",
                if *is_header { "" } else { " (Data)" }
            )),
            TagType::IndexBuffer { is_header } => f.write_fmt(format_args!(
                "IndexBuffer{}",
                if *is_header { "" } else { " (Data)" }
            )),
            TagType::ConstantBuffer { is_header } => f.write_fmt(format_args!(
                "ConstantBuffer{}",
                if *is_header { "" } else { " (Data)" }
            )),
            TagType::PixelShader { is_header } => f.write_fmt(format_args!(
                "PixelShader{}",
                if *is_header { "" } else { " (Data)" }
            )),
            TagType::VertexShader { is_header } => f.write_fmt(format_args!(
                "VertexShader{}",
                if *is_header { "" } else { " (Data)" }
            )),
            TagType::GeometryShader { is_header } => f.write_fmt(format_args!(
                "GeometryShader{}",
                if *is_header { "" } else { " (Data)" }
            )),
            TagType::ComputeShader { is_header } => f.write_fmt(format_args!(
                "ComputeShader{}",
                if *is_header { "" } else { " (Data)" }
            )),
            TagType::Tag => f.write_str("Tag"),
            TagType::TagGlobal => f.write_str("TagGlobal"),
            TagType::WwiseBank => f.write_str("WwiseBank"),
            TagType::WwiseStream => f.write_str("WwiseStream"),
            TagType::Havok => f.write_str("Havok"),
            TagType::OtfFontOrUmbraTome => f.write_str("OTF Font / Umbra Tome"),
            TagType::CriwareUsm => f.write_str("CriwareUsm"),
            TagType::Unknown { ftype, fsubtype } => {
                f.write_fmt(format_args!("Unk{ftype}+{fsubtype}"))
            }
        }
    }
}

impl TagType {
    pub fn from_type_subtype(t: u8, st: u8) -> TagType {
        // TODO: Change this match to use ordered version checking after destiny-pkg 0.11
        match package_manager().version {
            GameVersion::DestinyInternalAlpha => Self::from_type_subtype_devalpha(t, st),
            GameVersion::DestinyTheTakenKing | GameVersion::DestinyRiseOfIron => {
                Self::from_type_subtype_d1(t, st)
            }
            GameVersion::Destiny2Beta
            | GameVersion::Destiny2Forsaken
            | GameVersion::Destiny2Shadowkeep => Self::from_type_subtype_sk(t, st),
            GameVersion::Destiny2BeyondLight
            | GameVersion::Destiny2WitchQueen
            | GameVersion::Destiny2Lightfall
            | GameVersion::Destiny2TheFinalShape => Self::from_type_subtype_lf(t, st),
        }
    }

    pub fn from_type_subtype_devalpha(t: u8, st: u8) -> TagType {
        match (t, st) {
            (0, 0) => TagType::Tag,
            (16, 0) => TagType::Tag,
            (128, 0) => TagType::TagGlobal,
            (0, 15) => TagType::WwiseBank,
            (2, 16) => TagType::WwiseStream,
            (32 | 64 | 1, _) => {
                let is_header = t == 32;
                match st {
                    1 => TagType::Texture2D { is_header },
                    2 => TagType::TextureCube { is_header },
                    3 => TagType::Texture3D { is_header },
                    4 => TagType::VertexBuffer { is_header },
                    6 => TagType::IndexBuffer { is_header },
                    // 7 => TagType::ConstantBuffer { is_header },
                    8 => TagType::PixelShader { is_header },
                    9 => TagType::VertexShader { is_header },
                    fsubtype => TagType::Unknown { ftype: t, fsubtype },
                }
            }
            (ftype, fsubtype) => TagType::Unknown { ftype, fsubtype },
        }
    }

    pub fn from_type_subtype_d1(t: u8, st: u8) -> TagType {
        match (t, st) {
            (0, 20) => TagType::WwiseBank,
            (8, 21) => TagType::WwiseStream,
            (16, 0) => TagType::Tag,
            (32 | 1, _) => {
                let is_header = t == 32;
                match st {
                    1 => TagType::Texture2D { is_header },
                    2 => TagType::TextureCube { is_header },
                    3 => TagType::Texture3D { is_header },
                    4 => TagType::VertexBuffer { is_header },
                    6 => TagType::IndexBuffer { is_header },
                    // 7 => TagType::ConstantBuffer { is_header },
                    8 => TagType::PixelShader { is_header },
                    9 => TagType::VertexShader { is_header },
                    fsubtype => TagType::Unknown { ftype: t, fsubtype },
                }
            }
            (128, 0) => TagType::TagGlobal,
            (ftype, fsubtype) => TagType::Unknown { ftype, fsubtype },
        }
    }

    pub fn from_type_subtype_sk(t: u8, st: u8) -> TagType {
        let is_header = matches!(t, 32..=34);

        match (t, st) {
            (8, 0) => TagType::Tag,
            (16, 0) => TagType::TagGlobal,
            (26, 5) => TagType::WwiseBank,
            (26, 6) => TagType::WwiseStream,
            (26, 7) => TagType::Havok,
            (27, 0) => TagType::CriwareUsm,
            (32 | 40, _) => match st {
                1 => TagType::Texture2D { is_header },
                2 => TagType::TextureCube { is_header },
                3 => TagType::Texture3D { is_header },
                4 => TagType::VertexBuffer { is_header },
                6 => TagType::IndexBuffer { is_header },
                7 => TagType::ConstantBuffer { is_header },
                fsubtype => TagType::Unknown { ftype: t, fsubtype },
            },
            (33 | 41, _) => match st {
                0 => TagType::PixelShader { is_header },
                1 => TagType::VertexShader { is_header },
                2 => TagType::GeometryShader { is_header },
                6 => TagType::ComputeShader { is_header },
                fsubtype => TagType::Unknown { ftype: t, fsubtype },
            },
            (34 | 42, _) => match st {
                1 => TagType::TextureSampler { is_header },
                fsubtype => TagType::Unknown { ftype: t, fsubtype },
            },
            (ftype, fsubtype) => TagType::Unknown { ftype, fsubtype },
        }
    }

    pub fn from_type_subtype_lf(t: u8, st: u8) -> TagType {
        let is_header = matches!(t, 32..=34);

        match (t, st) {
            (8, 0) => TagType::Tag,
            (16, 0) => TagType::TagGlobal,
            (24, 0) => TagType::OtfFontOrUmbraTome,
            (26, 6) => TagType::WwiseBank,
            (26, 7) => TagType::WwiseStream,
            (27, 0) => TagType::Havok,
            (27, 1) => TagType::CriwareUsm,
            (32 | 40, _) => match st {
                1 => TagType::Texture2D { is_header },
                2 => TagType::TextureCube { is_header },
                3 => TagType::Texture3D { is_header },
                4 => TagType::VertexBuffer { is_header },
                6 => TagType::IndexBuffer { is_header },
                7 => TagType::ConstantBuffer { is_header },
                fsubtype => TagType::Unknown { ftype: t, fsubtype },
            },
            (33 | 41, _) => match st {
                0 => TagType::PixelShader { is_header },
                1 => TagType::VertexShader { is_header },
                2 => TagType::GeometryShader { is_header },
                6 => TagType::ComputeShader { is_header },
                fsubtype => TagType::Unknown { ftype: t, fsubtype },
            },
            (34 | 42, _) => match st {
                1 => TagType::TextureSampler { is_header },
                fsubtype => TagType::Unknown { ftype: t, fsubtype },
            },
            (48, 1) => TagType::TextureLargeBuffer,
            (ftype, fsubtype) => TagType::Unknown { ftype, fsubtype },
        }
    }
}
