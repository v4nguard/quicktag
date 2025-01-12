use binrw::BinRead;
use eframe::wgpu;
use std::mem::transmute;

#[allow(non_camel_case_types, dead_code, clippy::upper_case_acronyms)]
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Hash, BinRead)]
#[br(repr(u32))]
pub enum DxgiFormat {
    Unknown = 0,
    R32G32B32A32_TYPELESS = 1,
    R32G32B32A32_FLOAT = 2,
    R32G32B32A32_UINT = 3,
    R32G32B32A32_SINT = 4,
    R32G32B32_TYPELESS = 5,
    R32G32B32_FLOAT = 6,
    R32G32B32_UINT = 7,
    R32G32B32_SINT = 8,
    R16G16B16A16_TYPELESS = 9,
    R16G16B16A16_FLOAT = 10,
    R16G16B16A16_UNORM = 11,
    R16G16B16A16_UINT = 12,
    R16G16B16A16_SNORM = 13,
    R16G16B16A16_SINT = 14,
    R32G32_TYPELESS = 15,
    R32G32_FLOAT = 16,
    R32G32_UINT = 17,
    R32G32_SINT = 18,
    R32G8X24_TYPELESS = 19,
    D32_FLOAT_S8X24_UINT = 20,
    R32_FLOAT_X8X24_TYPELESS = 21,
    X32_TYPELESS_G8X24_UINT = 22,
    R10G10B10A2_TYPELESS = 23,
    R10G10B10A2_UNORM = 24,
    R10G10B10A2_UINT = 25,
    R11G11B10_FLOAT = 26,
    R8G8B8A8_TYPELESS = 27,
    R8G8B8A8_UNORM = 28,
    R8G8B8A8_UNORM_SRGB = 29,
    R8G8B8A8_UINT = 30,
    R8G8B8A8_SNORM = 31,
    R8G8B8A8_SINT = 32,
    R16G16_TYPELESS = 33,
    R16G16_FLOAT = 34,
    R16G16_UNORM = 35,
    R16G16_UINT = 36,
    R16G16_SNORM = 37,
    R16G16_SINT = 38,
    R32_TYPELESS = 39,
    D32_FLOAT = 40,
    R32_FLOAT = 41,
    R32_UINT = 42,
    R32_SINT = 43,
    R24G8_TYPELESS = 44,
    D24_UNORM_S8_UINT = 45,
    R24_UNORM_X8_TYPELESS = 46,
    X24_TYPELESS_G8_UINT = 47,
    R8G8_TYPELESS = 48,
    R8G8_UNORM = 49,
    R8G8_UINT = 50,
    R8G8_SNORM = 51,
    R8G8_SINT = 52,
    R16_TYPELESS = 53,
    R16_FLOAT = 54,
    D16_UNORM = 55,
    R16_UNORM = 56,
    R16_UINT = 57,
    R16_SNORM = 58,
    R16_SINT = 59,
    R8_TYPELESS = 60,
    R8_UNORM = 61,
    R8_UINT = 62,
    R8_SNORM = 63,
    R8_SINT = 64,
    A8_UNORM = 65,
    R1_UNORM = 66,
    R9G9B9E5_SHAREDEXP = 67,
    R8G8_B8G8_UNORM = 68,
    G8R8_G8B8_UNORM = 69,
    BC1_TYPELESS = 70,
    BC1_UNORM = 71,
    BC1_UNORM_SRGB = 72,
    BC2_TYPELESS = 73,
    BC2_UNORM = 74,
    BC2_UNORM_SRGB = 75,
    BC3_TYPELESS = 76,
    BC3_UNORM = 77,
    BC3_UNORM_SRGB = 78,
    BC4_TYPELESS = 79,
    BC4_UNORM = 80,
    BC4_SNORM = 81,
    BC5_TYPELESS = 82,
    BC5_UNORM = 83,
    BC5_SNORM = 84,
    B5G6R5_UNORM = 85,
    B5G5R5A1_UNORM = 86,
    B8G8R8A8_UNORM = 87,
    B8G8R8X8_UNORM = 88,
    R10G10B10_XR_BIAS_A2_UNORM = 89,
    B8G8R8A8_TYPELESS = 90,
    B8G8R8A8_UNORM_SRGB = 91,
    B8G8R8X8_TYPELESS = 92,
    B8G8R8X8_UNORM_SRGB = 93,
    BC6H_TYPELESS = 94,
    BC6H_UF16 = 95,
    BC6H_SF16 = 96,
    BC7_TYPELESS = 97,
    BC7_UNORM = 98,
    BC7_UNORM_SRGB = 99,
    AYUV = 100,
    Y410 = 101,
    Y416 = 102,
    NV12 = 103,
    P010 = 104,
    P016 = 105,
    OPAQUE420 = 106,
    YUY2 = 107,
    Y210 = 108,
    Y216 = 109,
    NV11 = 110,
    AI44 = 111,
    IA44 = 112,
    P8 = 113,
    A8P8 = 114,
    B4G4R4A4_UNORM = 115,
    P208 = 130,
    V208 = 131,
    V408 = 132,
    SAMPLER_FEEDBACK_MIN_MIP_OPAQUE,
    SAMPLER_FEEDBACK_MIP_REGION_USED_OPAQUE,
    FORCE_UINT = 0xffffffff,
}

impl From<DxgiFormat> for u32 {
    fn from(val: DxgiFormat) -> Self {
        unsafe { transmute(val) }
    }
}

impl TryFrom<u32> for DxgiFormat {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(match value {
            0..=115 | 130..=132 => unsafe { transmute(value) },
            e => return Err(anyhow::anyhow!("DXGI format is out of range ({e})")),
        })
    }
}

impl DxgiFormat {
    pub fn to_wgpu(self) -> anyhow::Result<wgpu::TextureFormat> {
        Ok(match self {
            DxgiFormat::R32G32B32A32_TYPELESS => wgpu::TextureFormat::Rgba32Float,
            DxgiFormat::R32G32B32A32_FLOAT => wgpu::TextureFormat::Rgba32Float,
            DxgiFormat::R32G32B32A32_UINT => wgpu::TextureFormat::Rgba32Uint,
            DxgiFormat::R32G32B32A32_SINT => wgpu::TextureFormat::Rgba32Sint,
            // DxgiFormat::R32G32B32_TYPELESS => VkFormat::R32G32B32_SFLOAT,
            // DxgiFormat::R32G32B32_FLOAT => VkFormat::R32G32B32_SFLOAT,
            // DxgiFormat::R32G32B32_UINT => VkFormat::R32G32B32_UINT,
            // DxgiFormat::R32G32B32_SINT => VkFormat::R32G32B32_SINT,
            DxgiFormat::R16G16B16A16_TYPELESS => wgpu::TextureFormat::Rgba16Float,
            DxgiFormat::R16G16B16A16_FLOAT => wgpu::TextureFormat::Rgba16Float,
            DxgiFormat::R16G16B16A16_UNORM => wgpu::TextureFormat::Rgba16Unorm,
            DxgiFormat::R16G16B16A16_UINT => wgpu::TextureFormat::Rgba16Uint,
            DxgiFormat::R16G16B16A16_SNORM => wgpu::TextureFormat::Rgba16Snorm,
            DxgiFormat::R16G16B16A16_SINT => wgpu::TextureFormat::Rgba16Sint,
            // DxgiFormat::R32G32_TYPELESS => VkFormat::R32G32_SFLOAT,
            // DxgiFormat::R32G32_FLOAT => VkFormat::R32G32_SFLOAT,
            // DxgiFormat::R32G32_UINT => VkFormat::R32G32_UINT,
            // DxgiFormat::R32G32_SINT => VkFormat::R32G32_SINT,
            DxgiFormat::R10G10B10A2_TYPELESS => wgpu::TextureFormat::Rgb10a2Unorm,
            DxgiFormat::R10G10B10A2_UNORM => wgpu::TextureFormat::Rgb10a2Unorm,
            // DxgiFormat::R10G10B10A2_UINT => VkFormat::A2B10G10R10_UINT_PACK32,
            DxgiFormat::R11G11B10_FLOAT => wgpu::TextureFormat::Rg11b10Float,
            // DxgiFormat::R8G8_TYPELESS => VkFormat::R8G8_UNORM,
            // DxgiFormat::R8G8_UNORM => VkFormat::R8G8_UNORM,
            // DxgiFormat::R8G8_UINT => VkFormat::R8G8_UINT,
            // DxgiFormat::R8G8_SNORM => VkFormat::R8G8_SNORM,
            // DxgiFormat::R8G8_SINT => VkFormat::R8G8_SINT,
            DxgiFormat::R8G8B8A8_TYPELESS => wgpu::TextureFormat::Rgba8Unorm,
            DxgiFormat::R8G8B8A8_UNORM => wgpu::TextureFormat::Rgba8UnormSrgb, // cohae: Bungie interprets non-sRGB rgba8 as sRGB??
            DxgiFormat::R8G8B8A8_UNORM_SRGB => wgpu::TextureFormat::Rgba8UnormSrgb,
            DxgiFormat::R8G8B8A8_UINT => wgpu::TextureFormat::Rgba8Uint,
            DxgiFormat::R8G8B8A8_SNORM => wgpu::TextureFormat::Rgba8Snorm,
            DxgiFormat::R8G8B8A8_SINT => wgpu::TextureFormat::Rgba8Sint,
            DxgiFormat::R16G16_TYPELESS => wgpu::TextureFormat::Rg16Float,
            DxgiFormat::R16G16_FLOAT => wgpu::TextureFormat::Rg16Float,
            DxgiFormat::R16G16_UNORM => wgpu::TextureFormat::Rg16Unorm,
            DxgiFormat::R16G16_UINT => wgpu::TextureFormat::Rg16Uint,
            DxgiFormat::R16G16_SNORM => wgpu::TextureFormat::Rg16Snorm,
            DxgiFormat::R16G16_SINT => wgpu::TextureFormat::Rg16Sint,
            DxgiFormat::R32_TYPELESS => wgpu::TextureFormat::R32Float,
            DxgiFormat::D32_FLOAT => wgpu::TextureFormat::Depth32Float,
            DxgiFormat::R32_FLOAT => wgpu::TextureFormat::R32Float,
            DxgiFormat::R32_UINT => wgpu::TextureFormat::R32Uint,
            DxgiFormat::R32_SINT => wgpu::TextureFormat::R32Sint,
            DxgiFormat::R16_TYPELESS => wgpu::TextureFormat::R16Unorm,
            DxgiFormat::R16_FLOAT => wgpu::TextureFormat::R16Float,
            DxgiFormat::D16_UNORM => wgpu::TextureFormat::Depth16Unorm,
            DxgiFormat::R16_UNORM => wgpu::TextureFormat::R16Unorm,
            DxgiFormat::R16_UINT => wgpu::TextureFormat::R16Uint,
            DxgiFormat::R16_SNORM => wgpu::TextureFormat::R16Snorm,
            DxgiFormat::R16_SINT => wgpu::TextureFormat::R16Sint,
            DxgiFormat::R8_TYPELESS => wgpu::TextureFormat::R8Unorm,
            DxgiFormat::R8_UNORM => wgpu::TextureFormat::R8Unorm,
            DxgiFormat::R8_UINT => wgpu::TextureFormat::R8Uint,
            DxgiFormat::R8_SNORM => wgpu::TextureFormat::Rgba16Snorm,
            DxgiFormat::R8_SINT => wgpu::TextureFormat::R8Sint,
            DxgiFormat::A8_UNORM => wgpu::TextureFormat::R8Unorm,
            DxgiFormat::B8G8R8A8_UNORM => wgpu::TextureFormat::Bgra8Unorm,
            DxgiFormat::B8G8R8X8_UNORM => wgpu::TextureFormat::Bgra8Unorm,
            DxgiFormat::B8G8R8A8_TYPELESS => wgpu::TextureFormat::Bgra8Unorm,
            DxgiFormat::B8G8R8A8_UNORM_SRGB => wgpu::TextureFormat::Bgra8UnormSrgb,
            DxgiFormat::B8G8R8X8_TYPELESS => wgpu::TextureFormat::Bgra8Unorm,
            DxgiFormat::B8G8R8X8_UNORM_SRGB => wgpu::TextureFormat::Bgra8UnormSrgb,
            // DxgiFormat::R9G9B9E5_SHAREDEXP => VkFormat::E5B9G9R9_UFLOAT_PACK32,
            // DxgiFormat::B5G6R5_UNORM => VkFormat::R5G6B5_UNORM_PACK16,
            // DxgiFormat::B5G5R5A1_UNORM => VkFormat::A1R5G5B5_UNORM_PACK16,
            DxgiFormat::BC1_TYPELESS => wgpu::TextureFormat::Bc1RgbaUnorm,
            DxgiFormat::BC1_UNORM => wgpu::TextureFormat::Bc1RgbaUnorm,
            DxgiFormat::BC1_UNORM_SRGB => wgpu::TextureFormat::Bc1RgbaUnormSrgb,
            DxgiFormat::BC2_TYPELESS => wgpu::TextureFormat::Bc2RgbaUnorm,
            DxgiFormat::BC2_UNORM => wgpu::TextureFormat::Bc2RgbaUnorm,
            DxgiFormat::BC2_UNORM_SRGB => wgpu::TextureFormat::Bc2RgbaUnormSrgb,
            DxgiFormat::BC3_TYPELESS => wgpu::TextureFormat::Bc3RgbaUnorm,
            DxgiFormat::BC3_UNORM => wgpu::TextureFormat::Bc3RgbaUnorm,
            DxgiFormat::BC3_UNORM_SRGB => wgpu::TextureFormat::Bc3RgbaUnormSrgb,
            DxgiFormat::BC4_TYPELESS => wgpu::TextureFormat::Bc4RUnorm,
            DxgiFormat::BC4_UNORM => wgpu::TextureFormat::Bc4RUnorm,
            DxgiFormat::BC4_SNORM => wgpu::TextureFormat::Bc4RSnorm,
            DxgiFormat::BC5_TYPELESS => wgpu::TextureFormat::Bc5RgUnorm,
            DxgiFormat::BC5_UNORM => wgpu::TextureFormat::Bc5RgUnorm,
            DxgiFormat::BC5_SNORM => wgpu::TextureFormat::Bc5RgSnorm,
            DxgiFormat::BC6H_TYPELESS => wgpu::TextureFormat::Bc6hRgbUfloat,
            DxgiFormat::BC6H_UF16 => wgpu::TextureFormat::Bc6hRgbUfloat,
            DxgiFormat::BC6H_SF16 => wgpu::TextureFormat::Bc6hRgbFloat,
            DxgiFormat::BC7_TYPELESS => wgpu::TextureFormat::Bc7RgbaUnorm,
            DxgiFormat::BC7_UNORM => wgpu::TextureFormat::Bc7RgbaUnorm,
            DxgiFormat::BC7_UNORM_SRGB => wgpu::TextureFormat::Bc7RgbaUnormSrgb,
            // DxgiFormat::B4G4R4A4_UNORM => VkFormat::A4R4G4B4_UNORM_PACK16,
            u => anyhow::bail!("Unsupported DXGI format conversion ({u:?} => ??)"),
        })
    }

    pub fn bpp(&self) -> usize {
        match self {
            DxgiFormat::R32G32B32A32_TYPELESS
            | DxgiFormat::R32G32B32A32_FLOAT
            | DxgiFormat::R32G32B32A32_UINT
            | DxgiFormat::R32G32B32A32_SINT => 128,
            DxgiFormat::R32G32B32_TYPELESS
            | DxgiFormat::R32G32B32_FLOAT
            | DxgiFormat::R32G32B32_UINT
            | DxgiFormat::R32G32B32_SINT => 96,
            DxgiFormat::R16G16B16A16_TYPELESS
            | DxgiFormat::R16G16B16A16_FLOAT
            | DxgiFormat::R16G16B16A16_UNORM
            | DxgiFormat::R16G16B16A16_UINT
            | DxgiFormat::R16G16B16A16_SNORM
            | DxgiFormat::R16G16B16A16_SINT
            | DxgiFormat::R32G32_TYPELESS
            | DxgiFormat::R32G32_FLOAT
            | DxgiFormat::R32G32_UINT
            | DxgiFormat::R32G32_SINT
            | DxgiFormat::R32G8X24_TYPELESS
            | DxgiFormat::D32_FLOAT_S8X24_UINT
            | DxgiFormat::R32_FLOAT_X8X24_TYPELESS
            | DxgiFormat::X32_TYPELESS_G8X24_UINT
            | DxgiFormat::Y416
            | DxgiFormat::Y210
            | DxgiFormat::Y216 => 64,
            DxgiFormat::R10G10B10A2_TYPELESS
            | DxgiFormat::R10G10B10A2_UNORM
            | DxgiFormat::R10G10B10A2_UINT
            | DxgiFormat::R11G11B10_FLOAT
            | DxgiFormat::R8G8B8A8_TYPELESS
            | DxgiFormat::R8G8B8A8_UNORM
            | DxgiFormat::R8G8B8A8_UNORM_SRGB
            | DxgiFormat::R8G8B8A8_UINT
            | DxgiFormat::R8G8B8A8_SNORM
            | DxgiFormat::R8G8B8A8_SINT
            | DxgiFormat::R16G16_TYPELESS
            | DxgiFormat::R16G16_FLOAT
            | DxgiFormat::R16G16_UNORM
            | DxgiFormat::R16G16_UINT
            | DxgiFormat::R16G16_SNORM
            | DxgiFormat::R16G16_SINT
            | DxgiFormat::R32_TYPELESS
            | DxgiFormat::D32_FLOAT
            | DxgiFormat::R32_FLOAT
            | DxgiFormat::R32_UINT
            | DxgiFormat::R32_SINT
            | DxgiFormat::R24G8_TYPELESS
            | DxgiFormat::D24_UNORM_S8_UINT
            | DxgiFormat::R24_UNORM_X8_TYPELESS
            | DxgiFormat::X24_TYPELESS_G8_UINT
            | DxgiFormat::R9G9B9E5_SHAREDEXP
            | DxgiFormat::R8G8_B8G8_UNORM
            | DxgiFormat::G8R8_G8B8_UNORM
            | DxgiFormat::B8G8R8A8_UNORM
            | DxgiFormat::B8G8R8X8_UNORM
            | DxgiFormat::R10G10B10_XR_BIAS_A2_UNORM
            | DxgiFormat::B8G8R8A8_TYPELESS
            | DxgiFormat::B8G8R8A8_UNORM_SRGB
            | DxgiFormat::B8G8R8X8_TYPELESS
            | DxgiFormat::B8G8R8X8_UNORM_SRGB
            | DxgiFormat::AYUV
            | DxgiFormat::Y410
            | DxgiFormat::YUY2 => 32,
            DxgiFormat::P010 | DxgiFormat::P016 => 24,
            DxgiFormat::R8G8_TYPELESS
            | DxgiFormat::R8G8_UNORM
            | DxgiFormat::R8G8_UINT
            | DxgiFormat::R8G8_SNORM
            | DxgiFormat::R8G8_SINT
            | DxgiFormat::R16_TYPELESS
            | DxgiFormat::R16_FLOAT
            | DxgiFormat::D16_UNORM
            | DxgiFormat::R16_UNORM
            | DxgiFormat::R16_UINT
            | DxgiFormat::R16_SNORM
            | DxgiFormat::R16_SINT
            | DxgiFormat::B5G6R5_UNORM
            | DxgiFormat::B5G5R5A1_UNORM
            | DxgiFormat::A8P8
            | DxgiFormat::B4G4R4A4_UNORM => 16,
            DxgiFormat::NV12 | DxgiFormat::OPAQUE420 | DxgiFormat::NV11 => 12,
            DxgiFormat::R8_TYPELESS
            | DxgiFormat::R8_UNORM
            | DxgiFormat::R8_UINT
            | DxgiFormat::R8_SNORM
            | DxgiFormat::R8_SINT
            | DxgiFormat::A8_UNORM
            | DxgiFormat::AI44
            | DxgiFormat::IA44
            | DxgiFormat::P8 => 8,
            DxgiFormat::R1_UNORM => 1,
            DxgiFormat::BC1_TYPELESS
            | DxgiFormat::BC1_UNORM
            | DxgiFormat::BC1_UNORM_SRGB
            | DxgiFormat::BC4_TYPELESS
            | DxgiFormat::BC4_UNORM
            | DxgiFormat::BC4_SNORM => 4,
            DxgiFormat::BC2_TYPELESS
            | DxgiFormat::BC2_UNORM
            | DxgiFormat::BC2_UNORM_SRGB
            | DxgiFormat::BC3_TYPELESS
            | DxgiFormat::BC3_UNORM
            | DxgiFormat::BC3_UNORM_SRGB
            | DxgiFormat::BC5_TYPELESS
            | DxgiFormat::BC5_UNORM
            | DxgiFormat::BC5_SNORM
            | DxgiFormat::BC6H_TYPELESS
            | DxgiFormat::BC6H_UF16
            | DxgiFormat::BC6H_SF16
            | DxgiFormat::BC7_TYPELESS
            | DxgiFormat::BC7_UNORM
            | DxgiFormat::BC7_UNORM_SRGB => 8,
            u => panic!("{u:?}"),
        }
    }

    pub fn is_srgb(&self) -> bool {
        matches!(
            self,
            DxgiFormat::R8G8B8A8_UNORM_SRGB
                | DxgiFormat::BC1_UNORM_SRGB
                | DxgiFormat::BC2_UNORM_SRGB
                | DxgiFormat::BC3_UNORM_SRGB
                | DxgiFormat::B8G8R8A8_UNORM_SRGB
                | DxgiFormat::B8G8R8X8_UNORM_SRGB
                | DxgiFormat::BC7_UNORM_SRGB
        )
    }

    pub fn is_compressed(&self) -> bool {
        matches!(
            self,
            DxgiFormat::BC1_TYPELESS
                | DxgiFormat::BC1_UNORM
                | DxgiFormat::BC1_UNORM_SRGB
                | DxgiFormat::BC4_TYPELESS
                | DxgiFormat::BC4_UNORM
                | DxgiFormat::BC4_SNORM
                | DxgiFormat::BC2_TYPELESS
                | DxgiFormat::BC2_UNORM
                | DxgiFormat::BC2_UNORM_SRGB
                | DxgiFormat::BC3_TYPELESS
                | DxgiFormat::BC3_UNORM
                | DxgiFormat::BC3_UNORM_SRGB
                | DxgiFormat::BC5_TYPELESS
                | DxgiFormat::BC5_UNORM
                | DxgiFormat::BC5_SNORM
                | DxgiFormat::BC6H_TYPELESS
                | DxgiFormat::BC6H_UF16
                | DxgiFormat::BC6H_SF16
                | DxgiFormat::BC7_TYPELESS
                | DxgiFormat::BC7_UNORM
                | DxgiFormat::BC7_UNORM_SRGB
        )
    }

    pub fn calculate_pitch(&self, width: usize, height: usize) -> (usize, usize) {
        match *self {
            DxgiFormat::BC1_TYPELESS
            | DxgiFormat::BC1_UNORM
            | DxgiFormat::BC1_UNORM_SRGB
            | DxgiFormat::BC4_TYPELESS
            | DxgiFormat::BC4_UNORM
            | DxgiFormat::BC4_SNORM => {
                let nbw = ((width as i64 + 3) / 4).clamp(1, i64::MAX) as usize;
                let nbh = ((height as i64 + 3) / 4).clamp(1, i64::MAX) as usize;

                let pitch = nbw * 8;
                (pitch, pitch * nbh)
            }
            DxgiFormat::BC2_TYPELESS
            | DxgiFormat::BC2_UNORM
            | DxgiFormat::BC2_UNORM_SRGB
            | DxgiFormat::BC3_TYPELESS
            | DxgiFormat::BC3_UNORM
            | DxgiFormat::BC3_UNORM_SRGB
            | DxgiFormat::BC5_TYPELESS
            | DxgiFormat::BC5_UNORM
            | DxgiFormat::BC5_SNORM
            | DxgiFormat::BC6H_TYPELESS
            | DxgiFormat::BC6H_UF16
            | DxgiFormat::BC6H_SF16
            | DxgiFormat::BC7_TYPELESS
            | DxgiFormat::BC7_UNORM
            | DxgiFormat::BC7_UNORM_SRGB => {
                let nbw = ((width as i64 + 3) / 4).clamp(1, i64::MAX) as usize;
                let nbh = ((height as i64 + 3) / 4).clamp(1, i64::MAX) as usize;

                let pitch = nbw * 16;
                (pitch, pitch * nbh)
            }
            _ => {
                let pitch = (width * self.bpp() + 7) / 8;
                (pitch, height * pitch)
            }
        }
    }
}

// https://github.com/tge-was-taken/GFD-Studio/blob/master/GFDLibrary/Textures/GNF/SurfaceFormat.cs
#[allow(non_snake_case, non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u16)]
#[allow(dead_code)]
pub enum GcnSurfaceFormat {
    /// <summary>Invalid surface format.</summary>
    Invalid = 0x00000000,
    /// <summary>One 8-bit channel. X=0xFF</summary>
    Format8 = 0x00000001,
    /// <summary>One 16-bit channel. X=0xFFFF</summary>
    Format16 = 0x00000002,
    /// <summary>Two 8-bit channels. X=0x00FF, Y=0xFF00</summary>
    Format8_8 = 0x00000003,
    /// <summary>One 32-bit channel. X=0xFFFFFFFF</summary>
    Format32 = 0x00000004,
    /// <summary>Two 16-bit channels. X=0x0000FFFF, Y=0xFFFF0000</summary>
    Format16_16 = 0x00000005,
    /// <summary>One 10-bit channel (Z) and two 11-bit channels (Y,X). X=0x000007FF, Y=0x003FF800, Z=0xFFC00000 Interpreted only as floating-point by texture unit, but also as integer by rasterizer.</summary>
    Format10_11_11 = 0x00000006,
    /// <summary>Two 11-bit channels (Z,Y) and one 10-bit channel (X). X=0x000003FF, Y=0x001FFC00, Z=0xFFE00000 Interpreted only as floating-point by texture unit, but also as integer by rasterizer.</summary>
    Format11_11_10 = 0x00000007,
    /// <summary>Three 10-bit channels (W,Z,Y) and one 2-bit channel (X). X=0x00000003, Y=0x00000FFC, Z=0x003FF000, W=0xFFC00000 X is never negative, even when YZW are.</summary>
    Format10_10_10_2 = 0x00000008,
    /// <summary>One 2-bit channel (W) and three 10-bit channels (Z,Y,X). X=0x000003FF, Y=0x000FFC00, Z=0x3FF00000, W=0xC0000000 W is never negative, even when XYZ are.</summary>
    Format2_10_10_10 = 0x00000009,
    /// <summary>Four 8-bit channels. X=0x000000FF, Y=0x0000FF00, Z=0x00FF0000, W=0xFF000000</summary>
    Format8_8_8_8 = 0x0000000a,
    /// <summary>Two 32-bit channels.</summary>
    Format32_32 = 0x0000000b,
    /// <summary>Four 16-bit channels.</summary>
    Format16_16_16_16 = 0x0000000c,
    /// <summary>Three 32-bit channels.</summary>
    Format32_32_32 = 0x0000000d,
    /// <summary>Four 32-bit channels.</summary>
    Format32_32_32_32 = 0x0000000e,
    /// <summary>One 5-bit channel (Z), one 6-bit channel (Y), and a second 5-bit channel (X). X=0x001F, Y=0x07E0, Z=0xF800</summary>
    Format5_6_5 = 0x00000010,
    /// <summary>One 1-bit channel (W) and three 5-bit channels (Z,Y,X). X=0x001F, Y=0x03E0, Z=0x7C00, W=0x8000</summary>
    Format1_5_5_5 = 0x00000011,
    /// <summary>Three 5-bit channels (W,Z,Y) and one 1-bit channel (X). X=0x0001, Y=0x003E, Z=0x07C0, W=0xF800</summary>
    Format5_5_5_1 = 0x00000012,
    /// <summary>Four 4-bit channels. X=0x000F, Y=0x00F0, Z=0x0F00, W=0xF000</summary>
    Format4_4_4_4 = 0x00000013,
    /// <summary>One 8-bit channel and one 24-bit channel.</summary>
    Format8_24 = 0x00000014,
    /// <summary>One 24-bit channel and one 8-bit channel.</summary>
    Format24_8 = 0x00000015,
    /// <summary>One 24-bit channel, one 8-bit channel, and one 32-bit channel.</summary>
    FormatX24_8_32 = 0x00000016,
    /// <summary>To be documented.</summary>
    GbGr = 0x00000020,
    /// <summary>To be documented.</summary>
    BgRg = 0x00000021,
    /// <summary>One 5-bit channel (W) and three 9-bit channels (Z,Y,X). X=0x000001FF, Y=0x0003FE00, Z=0x07FC0000, W=0xF8000000. Interpreted only as three 9-bit denormalized mantissas, and one shared 5-bit exponent.</summary>
    Format5_9_9_9 = 0x00000022,
    /// <summary>BC1 block-compressed surface.</summary>
    BC1 = 0x00000023,
    /// <summary>BC2 block-compressed surface.</summary>
    BC2 = 0x00000024,
    /// <summary>BC3 block-compressed surface.</summary>
    BC3 = 0x00000025,
    /// <summary>BC4 block-compressed surface.</summary>
    BC4 = 0x00000026,
    /// <summary>BC5 block-compressed surface.</summary>
    BC5 = 0x00000027,
    /// <summary>BC6 block-compressed surface.</summary>
    BC6 = 0x00000028,
    /// <summary>BC7 block-compressed surface.</summary>
    BC7 = 0x00000029,
    // /// <summary>8 bits-per-element FMASK surface (2 samples, 1 fragment).</summary>
    // Fmask8_S2_F1 = 0x0000002C,
    // /// <summary>8 bits-per-element FMASK surface (4 samples, 1 fragment).</summary>
    // Fmask8_S4_F1 = 0x0000002D,
    // /// <summary>8 bits-per-element FMASK surface (8 samples, 1 fragment).</summary>
    // Fmask8_S8_F1 = 0x0000002E,
    // /// <summary>8 bits-per-element FMASK surface (2 samples, 2 fragments).</summary>
    // Fmask8_S2_F2 = 0x0000002F,
    // /// <summary>8 bits-per-element FMASK surface (8 samples, 2 fragments).</summary>
    // Fmask8_S4_F2 = 0x00000030,
    // /// <summary>8 bits-per-element FMASK surface (4 samples, 4 fragments).</summary>
    // Fmask8_S4_F4 = 0x00000031,
    // /// <summary>16 bits-per-element FMASK surface (16 samples, 1 fragment).</summary>
    // Fmask16_S16_F1 = 0x00000032,
    // /// <summary>16 bits-per-element FMASK surface (8 samples, 2 fragments).</summary>
    // Fmask16_S8_F2 = 0x00000033,
    // /// <summary>32 bits-per-element FMASK surface (16 samples, 2 fragments).</summary>
    // Fmask32_S16_F2 = 0x00000034,
    // /// <summary>32 bits-per-element FMASK surface (8 samples, 4 fragments).</summary>
    // Fmask32_S8_F4 = 0x00000035,
    // /// <summary>32 bits-per-element FMASK surface (8 samples, 8 fragments).</summary>
    // Fmask32_S8_F8 = 0x00000036,
    // /// <summary>64 bits-per-element FMASK surface (16 samples, 4 fragments).</summary>
    // Fmask64_S16_F4 = 0x00000037,
    // /// <summary>64 bits-per-element FMASK surface (16 samples, 8 fragments).</summary>
    // Fmask64_S16_F8 = 0x00000038,
    // /// <summary>Two 4-bit channels (Y,X). X=0x0F, Y=0xF0</summary>
    // Format4_4 = 0x00000039,
    // /// <summary>One 6-bit channel (Z) and two 5-bit channels (Y,X). X=0x001F, Y=0x03E0, Z=0xFC00</summary>
    // Format6_5_5 = 0x0000003A,
    // /// <summary>One 1-bit channel. 8 pixels per byte, with pixel index increasing from LSB to MSB.</summary>
    // Format1 = 0x0000003B,
    // /// <summary>One 1-bit channel. 8 pixels per byte, with pixel index increasing from MSB to LSB.</summary>
    // Format1Reversed = 0x0000003C,
}

impl GcnSurfaceFormat {
    pub fn to_wgpu(self) -> anyhow::Result<wgpu::TextureFormat> {
        Ok(match self {
            GcnSurfaceFormat::Format8 => wgpu::TextureFormat::R8Unorm,
            GcnSurfaceFormat::Format16 => wgpu::TextureFormat::R16Unorm,
            GcnSurfaceFormat::Format8_8 => wgpu::TextureFormat::Rg8Unorm,
            GcnSurfaceFormat::Format32 => wgpu::TextureFormat::R32Float,
            GcnSurfaceFormat::Format16_16 => wgpu::TextureFormat::Rg16Unorm,
            GcnSurfaceFormat::Format10_11_11 => wgpu::TextureFormat::Rg11b10Float,
            // GcnSurfaceFormat::Format11_11_10 => todo!(), // No wgpu equivalent
            GcnSurfaceFormat::Format10_10_10_2 => wgpu::TextureFormat::Rgb10a2Unorm,
            GcnSurfaceFormat::Format2_10_10_10 => wgpu::TextureFormat::Rgb10a2Unorm,
            GcnSurfaceFormat::Format8_8_8_8 => wgpu::TextureFormat::Rgba8UnormSrgb,
            GcnSurfaceFormat::Format32_32 => wgpu::TextureFormat::Rg32Float,
            GcnSurfaceFormat::Format16_16_16_16 => wgpu::TextureFormat::Rgba16Unorm,
            // GcnSurfaceFormat::Format32_32_32 => todo!(), // No wgpu equivalent
            GcnSurfaceFormat::Format32_32_32_32 => wgpu::TextureFormat::Rgba32Float,
            // GcnSurfaceFormat::Format5_6_5 => todo!(), // No wgpu equivalent
            // GcnSurfaceFormat::Format5_5_5_1 => todo!(), // No wgpu equivalent
            // GcnSurfaceFormat::Format5_5_5_1 => todo!(), // No wgpu equivalent
            // GcnSurfaceFormat::Format5_5_5_1 => todo!(), // No wgpu equivalent
            // GcnSurfaceFormat::Format4_4_4_4 => todo!(), // No wgpu equivalent
            GcnSurfaceFormat::Format8_24 => wgpu::TextureFormat::Depth24PlusStencil8,
            GcnSurfaceFormat::Invalid => todo!(),
            GcnSurfaceFormat::Format1_5_5_5 => todo!(),
            GcnSurfaceFormat::Format24_8 => todo!(),
            GcnSurfaceFormat::FormatX24_8_32 => todo!(),
            GcnSurfaceFormat::GbGr => todo!(),
            GcnSurfaceFormat::BgRg => todo!(),
            // GcnSurfaceFormat::Format5_9_9_9 => todo!(),
            GcnSurfaceFormat::BC1 => wgpu::TextureFormat::Bc1RgbaUnormSrgb,
            GcnSurfaceFormat::BC2 => wgpu::TextureFormat::Bc2RgbaUnormSrgb,
            GcnSurfaceFormat::BC3 => wgpu::TextureFormat::Bc3RgbaUnormSrgb,
            GcnSurfaceFormat::BC4 => wgpu::TextureFormat::Bc4RUnorm,
            GcnSurfaceFormat::BC5 => wgpu::TextureFormat::Bc5RgUnorm,
            GcnSurfaceFormat::BC6 => wgpu::TextureFormat::Bc6hRgbFloat,
            GcnSurfaceFormat::BC7 => wgpu::TextureFormat::Bc7RgbaUnormSrgb,
            u => anyhow::bail!("Unsupported GCN surface format conversion ({u:?} => ??)"),
        })
    }

    pub fn bpp(&self) -> usize {
        match self {
            GcnSurfaceFormat::Format8 => 8,
            GcnSurfaceFormat::Format16 => 16,
            GcnSurfaceFormat::Format8_8 => 16,
            GcnSurfaceFormat::Format32 => 32,
            GcnSurfaceFormat::Format16_16 => 32,
            GcnSurfaceFormat::Format10_11_11 => 32,
            GcnSurfaceFormat::Format10_10_10_2 => 32,
            GcnSurfaceFormat::Format2_10_10_10 => 32,
            GcnSurfaceFormat::Format8_8_8_8 => 32,
            GcnSurfaceFormat::Format32_32 => 64,
            GcnSurfaceFormat::Format16_16_16_16 => 64,
            GcnSurfaceFormat::Format32_32_32_32 => 128,
            GcnSurfaceFormat::Format8_24 => 32,
            GcnSurfaceFormat::Invalid => 0,
            GcnSurfaceFormat::Format1_5_5_5 => 16,
            GcnSurfaceFormat::Format24_8 => 32,
            GcnSurfaceFormat::FormatX24_8_32 => 64,
            GcnSurfaceFormat::GbGr => 0,
            GcnSurfaceFormat::BgRg => 0,
            GcnSurfaceFormat::BC1 | GcnSurfaceFormat::BC4 => 4,
            GcnSurfaceFormat::BC2
            | GcnSurfaceFormat::BC3
            | GcnSurfaceFormat::BC5
            | GcnSurfaceFormat::BC6
            | GcnSurfaceFormat::BC7 => 8,
            GcnSurfaceFormat::Format11_11_10 => 32,
            GcnSurfaceFormat::Format32_32_32 => 96,
            GcnSurfaceFormat::Format5_6_5 => 16,
            GcnSurfaceFormat::Format5_5_5_1 => 16,
            GcnSurfaceFormat::Format4_4_4_4 => 16,
            GcnSurfaceFormat::Format5_9_9_9 => 32,
        }
    }

    pub fn block_size(&self) -> usize {
        match self {
            GcnSurfaceFormat::BC1 | GcnSurfaceFormat::BC4 => 8,
            GcnSurfaceFormat::BC2
            | GcnSurfaceFormat::BC3
            | GcnSurfaceFormat::BC5
            | GcnSurfaceFormat::BC6
            | GcnSurfaceFormat::BC7 => 16,
            u => u.bpp() / 8,
        }
    }

    pub fn pixel_block_size(&self) -> usize {
        match self {
            GcnSurfaceFormat::BC1
            | GcnSurfaceFormat::BC2
            | GcnSurfaceFormat::BC3
            | GcnSurfaceFormat::BC4
            | GcnSurfaceFormat::BC5
            | GcnSurfaceFormat::BC6
            | GcnSurfaceFormat::BC7 => 4,
            _ => 1,
        }
    }

    pub fn is_compressed(&self) -> bool {
        matches!(
            self,
            GcnSurfaceFormat::BC1
                | GcnSurfaceFormat::BC2
                | GcnSurfaceFormat::BC3
                | GcnSurfaceFormat::BC4
                | GcnSurfaceFormat::BC5
                | GcnSurfaceFormat::BC6
                | GcnSurfaceFormat::BC7
        )
    }
}

impl TryFrom<u16> for GcnSurfaceFormat {
    type Error = anyhow::Error;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Ok(match value {
            0..=0x29 => unsafe { transmute(value) },
            e => return Err(anyhow::anyhow!("GCN format is out of range ({e})")),
        })
    }
}

#[allow(non_snake_case, non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
#[allow(dead_code)]
pub enum XenosSurfaceFormat {
    k_1_REVERSE = 0,
    k_1 = 1,
    k_8 = 2,
    k_1_5_5_5 = 3,
    k_5_6_5 = 4,
    k_6_5_5 = 5,
    k_8_8_8_8 = 6,
    k_2_10_10_10 = 7,
    k_8_A = 8,
    k_8_B = 9,
    k_8_8 = 10,
    k_Cr_Y1_Cb_Y0_REP = 11,
    k_Y1_Cr_Y0_Cb_REP = 12,
    k_16_16_EDRAM = 13,
    k_8_8_8_8_A = 14,
    k_4_4_4_4 = 15,
    k_10_11_11 = 16,
    k_11_11_10 = 17,
    k_DXT1 = 18,
    k_DXT2_3 = 19,
    k_DXT4_5 = 20,
    k_16_16_16_16_EDRAM = 21,
    k_24_8 = 22,
    k_24_8_FLOAT = 23,
    k_16 = 24,
    k_16_16 = 25,
    k_16_16_16_16 = 26,
    k_16_EXPAND = 27,
    k_16_16_EXPAND = 28,
    k_16_16_16_16_EXPAND = 29,
    k_16_FLOAT = 30,
    k_16_16_FLOAT = 31,
    k_16_16_16_16_FLOAT = 32,
    k_32 = 33,
    k_32_32 = 34,
    k_32_32_32_32 = 35,
    k_32_FLOAT = 36,
    k_32_32_FLOAT = 37,
    k_32_32_32_32_FLOAT = 38,
    k_32_AS_8 = 39,
    k_32_AS_8_8 = 40,
    k_16_MPEG = 41,
    k_16_16_MPEG = 42,
    k_8_INTERLACED = 43,
    k_32_AS_8_INTERLACED = 44,
    k_32_AS_8_8_INTERLACED = 45,
    k_16_INTERLACED = 46,
    k_16_MPEG_INTERLACED = 47,
    k_16_16_MPEG_INTERLACED = 48,
    k_DXN = 49,
    k_8_8_8_8_AS_16_16_16_16 = 50,
    k_DXT1_AS_16_16_16_16 = 51,
    k_DXT2_3_AS_16_16_16_16 = 52,
    k_DXT4_5_AS_16_16_16_16 = 53,
    k_2_10_10_10_AS_16_16_16_16 = 54,
    k_10_11_11_AS_16_16_16_16 = 55,
    k_11_11_10_AS_16_16_16_16 = 56,
    k_32_32_32_FLOAT = 57,
    k_DXT3A = 58,
    k_DXT5A = 59,
    k_CTX1 = 60,
    k_DXT3A_AS_1_1_1_1 = 61,
    k_8_8_8_8_GAMMA_EDRAM = 62,
    k_2_10_10_10_FLOAT_EDRAM = 63,
}

pub struct FormatInfo {
    // wgpu_format: Option<wgpu::TextureFormat>,
    pub block_width: u32,
    pub block_height: u32,
    pub bits_per_pixel: u32,
}

impl FormatInfo {
    pub fn bytes_per_block(&self) -> u32 {
        self.block_width * self.block_height * self.bits_per_pixel / 8
    }
}

macro_rules! format_info {
    ($block_width:expr, $block_height:expr, $bpp:expr) => {
        FormatInfo {
            // wgpu_format: Some(wgpu::TextureFormat::$wgpu_format),
            block_width: $block_width,
            block_height: $block_height,
            bits_per_pixel: $bpp,
        }
    };
}

impl XenosSurfaceFormat {
    pub fn to_wgpu(self) -> anyhow::Result<wgpu::TextureFormat> {
        Ok(match self {
            // XenosSurfaceFormat::k_1_REVERSE => wgpu::TextureFormat::UNKNOWN,
            // XenosSurfaceFormat::k_1 => wgpu::TextureFormat::UNKNOWN,
            XenosSurfaceFormat::k_8 => wgpu::TextureFormat::R8Unorm,
            // XenosSurfaceFormat::k_1_5_5_5 => wgpu::TextureFormat::UNKNOWN,
            // XenosSurfaceFormat::k_5_6_5 => wgpu::TextureFormat::UNKNOWN,
            // XenosSurfaceFormat::k_6_5_5 => wgpu::TextureFormat::UNKNOWN,
            XenosSurfaceFormat::k_8_8_8_8 => wgpu::TextureFormat::Rgba8UnormSrgb,
            XenosSurfaceFormat::k_2_10_10_10 => wgpu::TextureFormat::Rgb10a2Unorm,
            XenosSurfaceFormat::k_8_A => wgpu::TextureFormat::R8Unorm,
            XenosSurfaceFormat::k_8_B => wgpu::TextureFormat::R8Unorm,
            XenosSurfaceFormat::k_8_8 => wgpu::TextureFormat::Rg8Unorm,
            XenosSurfaceFormat::k_8_8_8_8_A => wgpu::TextureFormat::Rgba8Unorm,
            // XenosSurfaceFormat::k_4_4_4_4 => wgpu::TextureFormat::UNKNOWN,
            // XenosSurfaceFormat::k_10_11_11 => wgpu::TextureFormat::UNKNOWN,
            // XenosSurfaceFormat::k_11_11_10 => wgpu::TextureFormat::UNKNOWN,
            XenosSurfaceFormat::k_DXT1 => wgpu::TextureFormat::Bc1RgbaUnorm,
            XenosSurfaceFormat::k_DXT2_3 => wgpu::TextureFormat::Bc2RgbaUnorm,
            XenosSurfaceFormat::k_DXT4_5 => wgpu::TextureFormat::Bc3RgbaUnorm,
            XenosSurfaceFormat::k_16_16_16_16_EDRAM => wgpu::TextureFormat::Rgba16Unorm,
            // XenosSurfaceFormat::k_24_8 => wgpu::TextureFormat::UNKNOWN,
            // XenosSurfaceFormat::k_24_8_FLOAT => wgpu::TextureFormat::UNKNOWN,
            XenosSurfaceFormat::k_16 => wgpu::TextureFormat::R16Unorm,
            XenosSurfaceFormat::k_16_16 => wgpu::TextureFormat::Rg16Unorm,
            XenosSurfaceFormat::k_16_16_16_16 => wgpu::TextureFormat::Rgba16Unorm,
            XenosSurfaceFormat::k_16_EXPAND => wgpu::TextureFormat::R16Unorm,
            XenosSurfaceFormat::k_16_16_EXPAND => wgpu::TextureFormat::Rg16Unorm,
            XenosSurfaceFormat::k_16_16_16_16_EXPAND => wgpu::TextureFormat::Rgba16Unorm,
            XenosSurfaceFormat::k_16_FLOAT => wgpu::TextureFormat::R16Float,
            XenosSurfaceFormat::k_16_16_FLOAT => wgpu::TextureFormat::Rg16Float,
            XenosSurfaceFormat::k_16_16_16_16_FLOAT => wgpu::TextureFormat::Rgba16Float,
            XenosSurfaceFormat::k_DXN => wgpu::TextureFormat::Bc5RgUnorm,
            XenosSurfaceFormat::k_8_8_8_8_AS_16_16_16_16 => wgpu::TextureFormat::R8Unorm,
            XenosSurfaceFormat::k_DXT1_AS_16_16_16_16 => wgpu::TextureFormat::Bc1RgbaUnorm,
            XenosSurfaceFormat::k_DXT2_3_AS_16_16_16_16 => wgpu::TextureFormat::Bc2RgbaUnorm,
            XenosSurfaceFormat::k_DXT4_5_AS_16_16_16_16 => wgpu::TextureFormat::Bc3RgbaUnorm,
            XenosSurfaceFormat::k_DXT3A => wgpu::TextureFormat::Bc2RgbaUnorm,
            XenosSurfaceFormat::k_DXT5A => wgpu::TextureFormat::Bc4RUnorm,
            XenosSurfaceFormat::k_CTX1 => wgpu::TextureFormat::Rg8Unorm,
            XenosSurfaceFormat::k_DXT3A_AS_1_1_1_1 => wgpu::TextureFormat::Bc2RgbaUnorm,
            u => anyhow::bail!("Unsupported Xenos surface format conversion ({u:?} => ??)"),
        })
    }

    // https://github.com/xenia-project/xenia/blob/3d30b2eec3ab1f83140b09745bee881fb5d5dde2/src/xenia/gpu/texture_info_formats.cc
    pub fn format_info(&self) -> FormatInfo {
        match self {
            XenosSurfaceFormat::k_1_REVERSE => format_info!(1, 1, 1),
            XenosSurfaceFormat::k_1 => format_info!(1, 1, 1),
            XenosSurfaceFormat::k_8 => format_info!(1, 1, 8),
            XenosSurfaceFormat::k_1_5_5_5 => format_info!(1, 1, 16),
            XenosSurfaceFormat::k_5_6_5 => format_info!(1, 1, 16),
            XenosSurfaceFormat::k_6_5_5 => format_info!(1, 1, 16),
            XenosSurfaceFormat::k_8_8_8_8 => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_2_10_10_10 => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_8_A => format_info!(1, 1, 8),
            XenosSurfaceFormat::k_8_B => format_info!(1, 1, 8),
            XenosSurfaceFormat::k_8_8 => format_info!(1, 1, 16),
            XenosSurfaceFormat::k_Cr_Y1_Cb_Y0_REP => format_info!(2, 1, 16),
            XenosSurfaceFormat::k_Y1_Cr_Y0_Cb_REP => format_info!(2, 1, 16),
            XenosSurfaceFormat::k_16_16_EDRAM => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_8_8_8_8_A => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_4_4_4_4 => format_info!(1, 1, 16),
            XenosSurfaceFormat::k_10_11_11 => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_11_11_10 => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_DXT1 => format_info!(4, 4, 4),
            XenosSurfaceFormat::k_DXT2_3 => format_info!(4, 4, 8),
            XenosSurfaceFormat::k_DXT4_5 => format_info!(4, 4, 8),
            XenosSurfaceFormat::k_16_16_16_16_EDRAM => format_info!(1, 1, 64),
            XenosSurfaceFormat::k_24_8 => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_24_8_FLOAT => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_16 => format_info!(1, 1, 16),
            XenosSurfaceFormat::k_16_16 => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_16_16_16_16 => format_info!(1, 1, 64),
            XenosSurfaceFormat::k_16_EXPAND => format_info!(1, 1, 16),
            XenosSurfaceFormat::k_16_16_EXPAND => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_16_16_16_16_EXPAND => format_info!(1, 1, 64),
            XenosSurfaceFormat::k_16_FLOAT => format_info!(1, 1, 16),
            XenosSurfaceFormat::k_16_16_FLOAT => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_16_16_16_16_FLOAT => format_info!(1, 1, 64),
            XenosSurfaceFormat::k_32 => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_32_32 => format_info!(1, 1, 64),
            XenosSurfaceFormat::k_32_32_32_32 => format_info!(1, 1, 128),
            XenosSurfaceFormat::k_32_FLOAT => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_32_32_FLOAT => format_info!(1, 1, 64),
            XenosSurfaceFormat::k_32_32_32_32_FLOAT => format_info!(1, 1, 128),
            XenosSurfaceFormat::k_32_AS_8 => format_info!(4, 1, 8),
            XenosSurfaceFormat::k_32_AS_8_8 => format_info!(2, 1, 16),
            XenosSurfaceFormat::k_16_MPEG => format_info!(1, 1, 16),
            XenosSurfaceFormat::k_16_16_MPEG => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_8_INTERLACED => format_info!(1, 1, 8),
            XenosSurfaceFormat::k_32_AS_8_INTERLACED => format_info!(4, 1, 8),
            XenosSurfaceFormat::k_32_AS_8_8_INTERLACED => format_info!(1, 1, 16),
            XenosSurfaceFormat::k_16_INTERLACED => format_info!(1, 1, 16),
            XenosSurfaceFormat::k_16_MPEG_INTERLACED => format_info!(1, 1, 16),
            XenosSurfaceFormat::k_16_16_MPEG_INTERLACED => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_DXN => format_info!(4, 4, 8),
            XenosSurfaceFormat::k_8_8_8_8_AS_16_16_16_16 => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_DXT1_AS_16_16_16_16 => format_info!(4, 4, 4),
            XenosSurfaceFormat::k_DXT2_3_AS_16_16_16_16 => format_info!(4, 4, 8),
            XenosSurfaceFormat::k_DXT4_5_AS_16_16_16_16 => format_info!(4, 4, 8),
            XenosSurfaceFormat::k_2_10_10_10_AS_16_16_16_16 => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_10_11_11_AS_16_16_16_16 => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_11_11_10_AS_16_16_16_16 => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_32_32_32_FLOAT => format_info!(1, 1, 96),
            XenosSurfaceFormat::k_DXT3A => format_info!(4, 4, 4),
            XenosSurfaceFormat::k_DXT5A => format_info!(4, 4, 4),
            XenosSurfaceFormat::k_CTX1 => format_info!(4, 4, 4),
            XenosSurfaceFormat::k_DXT3A_AS_1_1_1_1 => format_info!(4, 4, 4),
            XenosSurfaceFormat::k_8_8_8_8_GAMMA_EDRAM => format_info!(1, 1, 32),
            XenosSurfaceFormat::k_2_10_10_10_FLOAT_EDRAM => format_info!(1, 1, 32),
        }
    }

    pub fn bpp(&self) -> u32 {
        self.format_info().bits_per_pixel
    }

    pub fn bytes_per_block(&self) -> u32 {
        self.format_info().bytes_per_block()
    }
}

impl TryFrom<u8> for XenosSurfaceFormat {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            0..=63 => unsafe { transmute(value) },
            e => return Err(anyhow::anyhow!("Xenos format is out of range ({e})")),
        })
    }
}
