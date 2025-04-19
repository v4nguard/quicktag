pub mod cache;
mod capture;
mod dxgi;
mod headers_pc;
mod headers_ps;
mod headers_xbox;
mod swizzle;
pub use capture::capture_texture;

use anyhow::Context;
use binrw::BinReaderExt;
use dxgi::{GcmSurfaceFormat, GcnSurfaceFormat};
use eframe::egui_wgpu::RenderState;
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use eframe::wgpu::TextureDimension;
use headers_pc::TextureHeaderPC;
use headers_ps::{TextureHeaderD2Ps4, TextureHeaderPs3, TextureHeaderRoiPs4};
use headers_xbox::{TextureHeaderDevAlphaX360, TextureHeaderRoiXbox};
use image::{DynamicImage, GenericImageView};
use swizzle::swizzle_ps::{GcmDeswizzler, GcnDeswizzler};
use swizzle::swizzle_xbox::XenosDetiler;
use swizzle::Deswizzler;
use tiger_pkg::{package::PackagePlatform, GameVersion, TagHash};
use tiger_pkg::{package_manager, DestinyVersion, MarathonVersion};

#[derive(Debug)]
pub struct TextureHeaderGeneric {
    pub data_size: u32,
    pub format: wgpu::TextureFormat,
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub array_size: u16,
    pub large_buffer: Option<TagHash>,

    pub deswizzle: bool,
    pub psformat: Option<GcnSurfaceFormat>,
}

impl TryFrom<TextureHeaderD2Ps4> for TextureHeaderGeneric {
    type Error = anyhow::Error;

    fn try_from(v: TextureHeaderD2Ps4) -> Result<Self, Self::Error> {
        Ok(TextureHeaderGeneric {
            data_size: v.data_size,
            format: v.format.to_wgpu()?,
            width: v.width,
            height: v.height,
            depth: v.depth,
            array_size: v.array_size,
            large_buffer: v.large_buffer,

            deswizzle: (v.flags1 & 0xc00) != 0x400,
            psformat: Some(v.format),
        })
    }
}

impl TryFrom<TextureHeaderPC> for TextureHeaderGeneric {
    type Error = anyhow::Error;

    fn try_from(v: TextureHeaderPC) -> Result<Self, Self::Error> {
        Ok(TextureHeaderGeneric {
            data_size: v.data_size,
            format: v.format.to_wgpu()?,
            width: v.width,
            height: v.height,
            depth: v.depth,
            array_size: v.array_size,
            large_buffer: v.large_buffer,

            deswizzle: false,
            psformat: None,
        })
    }
}

pub struct Texture {
    pub view: wgpu::TextureView,
    pub handle: wgpu::Texture,
    pub full_cubemap_texture: Option<wgpu::Texture>,
    pub aspect_ratio: f32,
    pub desc: TextureDesc,

    pub comment: Option<String>,
}

pub struct TextureDesc {
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_size: u32,
    /// Should the alpha channel be pre-multiplied on creation?
    pub premultiply_alpha: bool,
}

impl TextureDesc {
    pub fn info(&self) -> String {
        let cubemap = if self.array_size == 6 {
            " (cubemap)"
        } else {
            ""
        };
        format!(
            "{}x{}x{} {:?}{cubemap}",
            self.width, self.height, self.depth, self.format
        )
    }

    pub fn kind(&self) -> TextureType {
        if self.array_size == 6 {
            TextureType::TextureCube
        } else if self.depth > 1 {
            TextureType::Texture3D
        } else {
            TextureType::Texture2D
        }
    }
}

impl Texture {
    pub fn load_data_d2(
        hash: TagHash,
        load_full_mip: bool,
    ) -> anyhow::Result<(TextureHeaderGeneric, Vec<u8>, String)> {
        let texture_header_ref = package_manager()
            .get_entry(hash)
            .context("Texture header entry not found")?
            .reference;

        let header_data = package_manager()
            .read_tag(hash)
            .context("Failed to read texture header")?;

        let is_prebl = matches!(package_manager().version, GameVersion::Destiny(v) if v.is_prebl());

        let mut cur = std::io::Cursor::new(header_data);
        let texture: TextureHeaderGeneric = match package_manager().platform {
            PackagePlatform::PS4 => {
                let texheader: TextureHeaderD2Ps4 = cur.read_le_args((is_prebl,))?;
                TextureHeaderGeneric::try_from(texheader)?
            }
            PackagePlatform::Win64 => {
                let texheader: TextureHeaderPC = cur.read_le_args((is_prebl,))?;
                TextureHeaderGeneric::try_from(texheader)?
            }
            _ => unreachable!("Unsupported platform for D2 textures"),
        };
        let mut texture_data = if let Some(t) = texture.large_buffer {
            package_manager()
                .read_tag(t)
                .context("Failed to read texture data")?
        } else {
            package_manager()
                .read_tag(texture_header_ref)
                .context("Failed to read texture data")?
                .to_vec()
        };

        if load_full_mip && texture.large_buffer.is_some() {
            let ab = package_manager()
                .read_tag(texture_header_ref)
                .context("Failed to read large texture buffer")?
                .to_vec();

            texture_data.extend(ab);
        }

        let comment = format!("{texture:#X?}");

        match package_manager().platform {
            PackagePlatform::PS4 => {
                if texture.psformat.is_none() {
                    anyhow::bail!("Texture data not found: psformat: {:?}", texture.psformat);
                }
                let psformat = texture.psformat.unwrap();
                let expected_size =
                    (texture.width as usize * texture.height as usize * psformat.bpp()) / 8;

                if texture_data.len() < expected_size {
                    anyhow::bail!(
                        "Texture data size mismatch for {hash} ({}x{}x{} {:?}): expected {expected_size}, got {}",
                        texture.width, texture.height, texture.depth, texture.format,
                        texture_data.len()
                    );
                }

                if texture.deswizzle {
                    let unswizzled = GcnDeswizzler
                        .deswizzle(
                            &texture_data,
                            texture.width as usize,
                            texture.height as usize,
                            if texture.array_size > 1 {
                                texture.array_size as usize
                            } else {
                                texture.depth as usize
                            },
                            texture.psformat.unwrap(),
                            false,
                        )
                        .context("Failed to deswizzle texture")?;
                    Ok((texture, unswizzled, comment))
                } else {
                    Ok((texture, texture_data, comment))
                }
            }
            _ => Ok((texture, texture_data, comment)),
        }
    }

    pub fn load_data_roi_ps4(
        hash: TagHash,
        _load_full_mip: bool,
    ) -> anyhow::Result<(TextureHeaderRoiPs4, Vec<u8>, String)> {
        let texture_header_ref = package_manager()
            .get_entry(hash)
            .context("Texture header entry not found")?
            .reference;

        let texture: TextureHeaderRoiPs4 = package_manager().read_tag_binrw(hash)?;

        let large_buffer = package_manager()
            .get_entry(texture_header_ref)
            .map(|v| TagHash(v.reference))
            .unwrap_or_default();

        let texture_data = if large_buffer.is_some() {
            package_manager()
                .read_tag(large_buffer)
                .context("Failed to read texture data")?
        } else {
            package_manager()
                .read_tag(texture_header_ref)
                .context("Failed to read texture data")?
                .to_vec()
        };

        let expected_size =
            (texture.width as usize * texture.height as usize * texture.format.bpp()) / 8;

        if texture_data.len() < expected_size {
            anyhow::bail!(
                "Texture data size mismatch for {hash} ({}x{}x{} {:?}): expected {expected_size}, got {}",
                texture.width, texture.height, texture.depth, texture.format,
                texture_data.len()
            );
        }

        let comment = format!("{texture:#X?}");
        if (texture.flags1 & 0xc00) != 0x400 {
            let unswizzled = GcnDeswizzler
                .deswizzle(
                    &texture_data,
                    texture.width as usize,
                    texture.height as usize,
                    if texture.array_size > 1 {
                        texture.array_size as usize
                    } else {
                        texture.depth as usize
                    },
                    texture.format,
                    true,
                )
                .context("Failed to deswizzle texture")?;

            Ok((texture, unswizzled, comment))
        } else {
            Ok((texture, texture_data, comment))
        }
    }

    pub fn load_data_devalpha_x360(
        hash: TagHash,
        _load_full_mip: bool,
    ) -> anyhow::Result<(TextureHeaderDevAlphaX360, Vec<u8>, String)> {
        let texture_header_ref = package_manager()
            .get_entry(hash)
            .context("Texture header entry not found")?
            .reference;

        let texture: TextureHeaderDevAlphaX360 = package_manager().read_tag_binrw(hash)?;

        let large_buffer = package_manager()
            .get_entry(texture_header_ref)
            .map(|v| TagHash(v.reference))
            .unwrap_or_default();

        let texture_data = if large_buffer.is_some() {
            package_manager()
                .read_tag(large_buffer)
                .context("Failed to read texture data")?
        } else {
            package_manager()
                .read_tag(texture_header_ref)
                .context("Failed to read texture data")?
                .to_vec()
        };

        let expected_size =
            (texture.width as usize * texture.height as usize * texture.format.bpp() as usize) / 8;

        if texture_data.len() < expected_size {
            anyhow::bail!(
                "Texture data size mismatch for {hash} ({}x{}x{} {:?}): expected {expected_size}, got {}",
                texture.width, texture.height, texture.depth, texture.format,
                texture_data.len()
            );
        }

        let comment = format!("{texture:#X?}");

        let untiled = XenosDetiler
            .deswizzle(
                &texture_data,
                texture.width as usize,
                texture.height as usize,
                if texture.array_size > 1 {
                    texture.array_size as usize
                } else {
                    texture.depth as usize
                },
                texture.format,
                false,
            )
            .context("Failed to deswizzle texture")?;

        Ok((texture, untiled, comment))
    }

    pub fn load_data_roi_xone(
        hash: TagHash,
        _load_full_mip: bool,
    ) -> anyhow::Result<(TextureHeaderRoiXbox, Vec<u8>, String)> {
        let texture_header_ref = package_manager()
            .get_entry(hash)
            .context("Texture header entry not found")?
            .reference;

        let texture: TextureHeaderRoiXbox = package_manager().read_tag_binrw(hash)?;

        let large_buffer = package_manager()
            .get_entry(texture_header_ref)
            .map(|v| TagHash(v.reference))
            .unwrap_or_default();

        let texture_data = if large_buffer.is_some() {
            package_manager()
                .read_tag(large_buffer)
                .context("Failed to read texture data")?
        } else {
            package_manager()
                .read_tag(texture_header_ref)
                .context("Failed to read texture data")?
                .to_vec()
        };

        let expected_size =
            (texture.width as usize * texture.height as usize * texture.format.bpp()) / 8;

        if texture_data.len() < expected_size {
            anyhow::bail!(
                "Texture data size mismatch for {hash} ({}x{}x{} {:?}): expected {expected_size}, got {}",
                texture.width, texture.height, texture.depth, texture.format,
                texture_data.len()
            );
        }

        let comment = format!("{texture:#X?}");
        // if (texture.flags1 & 0xc00) != 0x400 {
        //     let mut unswizzled = vec![];
        //     swizzle::ps4::unswizzle(
        //         &texture_data,
        //         &mut unswizzled,
        //         texture.width as usize,
        //         texture.height as usize,
        //         texture.format,
        //     );
        //     Ok((texture, unswizzled, comment))
        // } else {
        // Ok((texture, texture_data, comment))
        // }

        // let mut untiled = vec![];
        // swizzle::xbox::untile(
        //     &texture_data,
        //     &mut untiled,
        //     texture.width as usize,
        //     texture.height as usize,
        //     texture.format,
        // );
        Ok((texture, texture_data, comment))
    }

    pub fn load_data_ps3_ttk(
        hash: TagHash,
        _load_full_mip: bool,
    ) -> anyhow::Result<(TextureHeaderPs3, Vec<u8>, String)> {
        let texture_header_ref = package_manager()
            .get_entry(hash)
            .context("Texture header entry not found")?
            .reference;

        let texture: TextureHeaderPs3 = package_manager().read_tag_binrw(hash)?;

        let large_buffer = package_manager()
            .get_entry(texture_header_ref)
            .map(|v| TagHash(v.reference))
            .unwrap_or_default();

        let texture_data = if large_buffer.is_some() {
            package_manager()
                .read_tag(large_buffer)
                .context("Failed to read texture data")?
        } else {
            package_manager()
                .read_tag(texture_header_ref)
                .context("Failed to read texture data")?
                .to_vec()
        };

        let expected_size =
            (texture.width as usize * texture.height as usize * texture.format.bpp()) / 8;

        if texture_data.len() < expected_size {
            anyhow::bail!(
                "Texture data size mismatch for {hash} ({}x{}x{} {:?}): expected {expected_size}, got {}",
                texture.width, texture.height, texture.depth, texture.format,
                texture_data.len()
            );
        }

        let mut data = texture_data.clone();
        let comment = format!("{texture:#X?}");

        if (texture.format as u8 & 0x20) != 0
            || (texture.format == GcmSurfaceFormat::A8R8G8B8 && texture.flags1 == 0)
            || texture.format == GcmSurfaceFormat::B8
        {
            data = GcmDeswizzler
                .deswizzle(
                    &texture_data,
                    texture.width as usize,
                    texture.height as usize,
                    if texture.array_size > 1 {
                        texture.array_size as usize
                    } else {
                        texture.depth as usize
                    },
                    texture.format,
                    false,
                )
                .context("Failed to deswizzle texture")?;
        }

        let unswizzled = GcmDeswizzler::color_deswizzle(&data, texture.format);
        Ok((texture, unswizzled, comment))
    }

    pub fn load_desc(hash: TagHash) -> anyhow::Result<TextureDesc> {
        match package_manager().version {
            GameVersion::Destiny(
                DestinyVersion::DestinyInternalAlpha | DestinyVersion::DestinyTheTakenKing,
            ) => match package_manager().platform {
                PackagePlatform::X360 => {
                    let texture: TextureHeaderDevAlphaX360 =
                        package_manager().read_tag_binrw(hash)?;
                    Ok(TextureDesc {
                        format: texture.format.to_wgpu()?,
                        width: texture.width as u32,
                        height: texture.height as u32,
                        array_size: texture.array_size as u32,
                        depth: texture.depth as u32,
                        premultiply_alpha: false,
                    })
                }
                PackagePlatform::PS3 => {
                    let texture: TextureHeaderPs3 = package_manager().read_tag_binrw(hash)?;
                    Ok(TextureDesc {
                        format: texture.format.to_wgpu()?,
                        width: texture.width as u32,
                        height: texture.height as u32,
                        array_size: texture.array_size as u32,
                        depth: texture.depth as u32,
                        premultiply_alpha: false,
                    })
                }
                _ => unreachable!("Unsupported platform for legacy D1 textures"),
            },
            GameVersion::Destiny(
                DestinyVersion::DestinyFirstLookAlpha | DestinyVersion::DestinyRiseOfIron,
            ) => match package_manager().platform {
                PackagePlatform::PS4 => {
                    let texture: TextureHeaderRoiPs4 = package_manager().read_tag_binrw(hash)?;
                    Ok(TextureDesc {
                        format: texture.format.to_wgpu()?,
                        width: texture.width as u32,
                        height: texture.height as u32,
                        array_size: texture.array_size as u32,
                        depth: texture.depth as u32,
                        premultiply_alpha: false,
                    })
                }
                PackagePlatform::XboxOne => {
                    let texture: TextureHeaderRoiXbox = package_manager().read_tag_binrw(hash)?;
                    Ok(TextureDesc {
                        format: texture.format.to_wgpu()?,
                        width: texture.width as u32,
                        height: texture.height as u32,
                        array_size: texture.array_size as u32,
                        depth: texture.depth as u32,
                        premultiply_alpha: false,
                    })
                }
                _ => unreachable!("Unsupported platform for RoI textures"),
            },
            GameVersion::Destiny(
                DestinyVersion::Destiny2Beta
                | DestinyVersion::Destiny2Forsaken
                | DestinyVersion::Destiny2Shadowkeep
                | DestinyVersion::Destiny2BeyondLight
                | DestinyVersion::Destiny2WitchQueen
                | DestinyVersion::Destiny2Lightfall
                | DestinyVersion::Destiny2TheFinalShape,
            )
            | GameVersion::Marathon(MarathonVersion::MarathonAlpha) => {
                let is_prebl =
                    matches!(package_manager().version, GameVersion::Destiny(v) if v.is_prebl());
                match package_manager().platform {
                    PackagePlatform::PS4 => {
                        let header_data = package_manager()
                            .read_tag(hash)
                            .context("Failed to read texture header")?;

                        let mut cur = std::io::Cursor::new(header_data);
                        let texture: TextureHeaderD2Ps4 = cur.read_le_args((is_prebl,))?;

                        Ok(TextureDesc {
                            format: texture.format.to_wgpu()?,
                            width: texture.width as u32,
                            height: texture.height as u32,
                            depth: texture.depth as u32,
                            array_size: texture.array_size as u32,
                            premultiply_alpha: false,
                        })
                    }
                    PackagePlatform::Win64 => {
                        let header_data = package_manager()
                            .read_tag(hash)
                            .context("Failed to read texture header")?;

                        let mut cur = std::io::Cursor::new(header_data);
                        let texture: TextureHeaderPC = cur.read_le_args((is_prebl,))?;

                        Ok(TextureDesc {
                            format: texture.format.to_wgpu()?,
                            width: texture.width as u32,
                            height: texture.height as u32,
                            depth: texture.depth as u32,
                            array_size: texture.array_size as u32,
                            premultiply_alpha: false,
                        })
                    }
                    _ => unreachable!("Unsupported platform for D2 textures"),
                }
            }
        }
    }

    pub fn load(
        rs: &RenderState,
        hash: TagHash,
        premultiply_alpha: bool,
    ) -> anyhow::Result<Texture> {
        match package_manager().version {
            GameVersion::Destiny(
                DestinyVersion::DestinyInternalAlpha | DestinyVersion::DestinyTheTakenKing,
            ) => match package_manager().platform {
                PackagePlatform::X360 => {
                    let (texture, texture_data, comment) =
                        Self::load_data_devalpha_x360(hash, true)?;
                    Self::create_texture(
                        rs,
                        hash,
                        TextureDesc {
                            format: texture.format.to_wgpu()?,
                            width: texture.width as u32,
                            height: texture.height as u32,
                            depth: texture.depth as u32,
                            array_size: texture.array_size as u32,
                            premultiply_alpha,
                        },
                        texture_data,
                        Some(comment),
                    )
                }
                PackagePlatform::PS3 => {
                    let (texture, texture_data, comment) = Self::load_data_ps3_ttk(hash, true)?;
                    Self::create_texture(
                        rs,
                        hash,
                        TextureDesc {
                            format: texture.format.to_wgpu()?,
                            width: texture.width as u32,
                            height: texture.height as u32,
                            depth: texture.depth as u32,
                            array_size: texture.array_size as u32,
                            premultiply_alpha,
                        },
                        texture_data,
                        Some(comment),
                    )
                }
                _ => anyhow::bail!("Unsupported platform for legacy D1 textures"),
            },
            GameVersion::Destiny(
                DestinyVersion::DestinyFirstLookAlpha | DestinyVersion::DestinyRiseOfIron,
            ) => {
                match package_manager().platform {
                    PackagePlatform::PS4 => {
                        let (texture, texture_data, comment) = Self::load_data_roi_ps4(hash, true)?;
                        Self::create_texture(
                            rs,
                            hash,
                            TextureDesc {
                                format: texture.format.to_wgpu()?,
                                width: texture.width as u32,
                                height: texture.height as u32,
                                depth: texture.depth as u32,
                                array_size: texture.array_size as u32,
                                premultiply_alpha,
                            },
                            texture_data,
                            Some(comment),
                        )
                    }
                    PackagePlatform::XboxOne => {
                        // anyhow::bail!("Xbox One textures are not supported yet");
                        let (texture, texture_data, comment) =
                            Self::load_data_roi_xone(hash, true)?;
                        Self::create_texture(
                            rs,
                            hash,
                            TextureDesc {
                                format: texture.format.to_wgpu()?,
                                width: texture.width as u32,
                                height: texture.height as u32,
                                depth: texture.depth as u32,
                                array_size: texture.array_size as u32,
                                premultiply_alpha,
                            },
                            texture_data,
                            Some(comment),
                        )
                    }
                    _ => unreachable!("Unsupported platform for RoI textures"),
                }
            }
            GameVersion::Destiny(
                DestinyVersion::Destiny2Beta
                | DestinyVersion::Destiny2Forsaken
                | DestinyVersion::Destiny2Shadowkeep
                | DestinyVersion::Destiny2BeyondLight
                | DestinyVersion::Destiny2WitchQueen
                | DestinyVersion::Destiny2Lightfall
                | DestinyVersion::Destiny2TheFinalShape,
            )
            | GameVersion::Marathon(MarathonVersion::MarathonAlpha) => {
                let (texture, texture_data, comment) = Self::load_data_d2(hash, true)?;
                Self::create_texture(
                    rs,
                    hash,
                    TextureDesc {
                        format: texture.format,
                        width: texture.width as u32,
                        height: texture.height as u32,
                        depth: texture.depth as u32,
                        array_size: texture.array_size as u32,
                        premultiply_alpha,
                    },
                    texture_data,
                    Some(comment),
                )
            }
        }
    }

    /// Create a wgpu texture from unswizzled texture data
    fn create_texture(
        rs: &RenderState,
        hash: TagHash,
        desc: TextureDesc,
        // cohae: Take ownership of the data so we don't have to clone it for premultiplication
        mut data: Vec<u8>,
        comment: Option<String>,
    ) -> anyhow::Result<Texture> {
        if desc.format.is_compressed() && desc.depth > 1 {
            anyhow::bail!("Compressed 3D textures are not supported by wgpu");
        }

        // Pre-multiply alpha where possible
        if matches!(
            desc.format,
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb
        ) {
            for c in data.chunks_exact_mut(4) {
                c[0] = (c[0] as f32 * c[3] as f32 / 255.) as u8;
                c[1] = (c[1] as f32 * c[3] as f32 / 255.) as u8;
                c[2] = (c[2] as f32 * c[3] as f32 / 255.) as u8;
            }
        }

        let image_size = wgpu::Extent3d {
            width: desc.width,
            height: desc.height,
            depth_or_array_layers: 1,
        };

        {
            let block_size = desc.format.block_copy_size(None).unwrap_or(4);
            let (block_width, block_height) = desc.format.block_dimensions();
            let physical_size = image_size.physical_size(desc.format);
            let width_blocks = physical_size.width / block_width;
            let height_blocks = physical_size.height / block_height;

            let bytes_per_row = width_blocks * block_size;
            let expected_data_size =
                bytes_per_row * height_blocks * image_size.depth_or_array_layers;

            anyhow::ensure!(
                data.len() >= expected_data_size as usize,
                "Not enough data for texture {hash} ({}): expected 0x{:X}, got 0x{:X}",
                desc.info(),
                expected_data_size,
                data.len()
            );
        }

        let handle = rs.device.create_texture_with_data(
            &rs.queue,
            &wgpu::TextureDescriptor {
                label: Some(&*format!("Texture {hash}")),
                size: wgpu::Extent3d {
                    depth_or_array_layers: 1,
                    ..image_size
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: desc.format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[desc.format],
            },
            wgpu::util::TextureDataOrder::default(),
            &data,
        );

        let view = handle.create_view(&wgpu::TextureViewDescriptor {
            ..Default::default()
        });

        let full_texture = if desc.array_size > 1 {
            let handle = rs.device.create_texture_with_data(
                &rs.queue,
                &wgpu::TextureDescriptor {
                    label: Some(&*format!("Texture {hash} (full)")),
                    size: wgpu::Extent3d {
                        depth_or_array_layers: desc.array_size,
                        ..image_size
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: desc.format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[desc.format],
                },
                wgpu::util::TextureDataOrder::default(),
                &data,
            );

            Some(handle)
        } else {
            None
        };

        Ok(Texture {
            view,
            handle,
            full_cubemap_texture: full_texture,
            aspect_ratio: desc.width as f32 / desc.height as f32,
            desc,
            comment,
        })
    }

    fn load_png(render_state: &RenderState, bytes: &[u8]) -> anyhow::Result<Texture> {
        let img = image::load_from_memory(bytes)?;
        let rgba = img.to_rgba8();
        let (width, height) = img.dimensions();
        Self::create_texture(
            render_state,
            TagHash::NONE,
            TextureDesc {
                format: wgpu::TextureFormat::Rgba8Unorm,
                width,
                height,
                array_size: 1,
                depth: 1,
                premultiply_alpha: true,
            },
            rgba.into_raw(),
            None,
        )
    }

    pub fn to_image(&self, rs: &RenderState, layer: u32) -> anyhow::Result<DynamicImage> {
        let (rgba_data, padded_width, padded_height) = capture_texture(rs, self, layer)?;
        let image = image::RgbaImage::from_raw(padded_width, padded_height, rgba_data)
            .context("Failed to create image")?;

        Ok(DynamicImage::from(image).crop(0, 0, self.desc.width, self.desc.height))
    }
}

#[derive(PartialEq)]
pub enum TextureType {
    Texture2D,
    Texture3D,
    TextureCube,
}
