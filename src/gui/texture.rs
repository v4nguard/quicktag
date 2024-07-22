use crate::gui::dxgi::DxgiFormat;
use crate::gui::texture::texture_capture::capture_texture;
use crate::package_manager::package_manager;
use anyhow::Context;
use binrw::{BinRead, BinReaderExt};
use destiny_pkg::package::PackagePlatform;
use destiny_pkg::{GameVersion, TagHash};
use eframe::egui::load::SizedTexture;
use eframe::egui_wgpu::RenderState;
use eframe::epaint::mutex::RwLock;
use eframe::epaint::{vec2, TextureId};
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use eframe::wgpu::TextureDimension;
use either::Either::{self, Left};
use image::{DynamicImage, GenericImageView};

use linked_hash_map::LinkedHashMap;
use poll_promise::Promise;
use rustc_hash::FxHasher;
use std::hash::BuildHasherDefault;
use std::io::SeekFrom;

use std::rc::Rc;
use std::sync::Arc;

use super::dxgi::GcnSurfaceFormat;

#[derive(Debug, BinRead)]
#[br(import(prebl: bool))]
pub struct TextureHeader {
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

pub struct Texture {
    pub view: wgpu::TextureView,
    pub handle: wgpu::Texture,
    pub format: wgpu::TextureFormat,
    pub aspect_ratio: f32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,

    pub comment: Option<String>,
}

pub struct TextureDesc {
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    /// Should the alpha channel be pre-multiplied on creation?
    pub premultiply_alpha: bool,
}

impl TextureDesc {
    pub fn info(&self) -> String {
        format!(
            "{}x{}x{} {:?}",
            self.width, self.height, self.depth, self.format
        )
    }
}

impl Texture {
    pub fn load_data_d2(
        hash: TagHash,
        load_full_mip: bool,
    ) -> anyhow::Result<(TextureHeader, Vec<u8>, String)> {
        let texture_header_ref = package_manager()
            .get_entry(hash)
            .context("Texture header entry not found")?
            .reference;

        let header_data = package_manager()
            .read_tag(hash)
            .context("Failed to read texture header")?;

        // TODO(cohae): add a method to GameVersion to check for prebl
        let is_prebl = matches!(
            package_manager().version,
            destiny_pkg::GameVersion::Destiny2Beta | destiny_pkg::GameVersion::Destiny2Shadowkeep
        );

        let mut cur = std::io::Cursor::new(header_data);
        let texture: TextureHeader = cur.read_le_args((is_prebl,))?;
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
        Ok((texture, texture_data, comment))
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
            let mut unswizzled = vec![];
            swizzle::ps4::unswizzle(
                &texture_data,
                &mut unswizzled,
                texture.width as usize,
                texture.height as usize,
                texture.format,
            );
            Ok((texture, unswizzled, comment))
        } else {
            Ok((texture, texture_data, comment))
        }
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

        let mut untiled = vec![];
        swizzle::xbox::untile(
            &texture_data,
            &mut untiled,
            texture.width as usize,
            texture.height as usize,
            texture.format,
        );
        Ok((texture, untiled, comment))
    }

    pub fn load_desc(hash: TagHash) -> anyhow::Result<TextureDesc> {
        if package_manager().version.is_d1()
            && !matches!(
                package_manager().platform,
                PackagePlatform::PS4 | PackagePlatform::XboxOne
            )
        {
            anyhow::bail!("Textures are not supported for D1");
        }

        match package_manager().version {
            GameVersion::DestinyInternalAlpha | GameVersion::DestinyTheTakenKing => todo!(),
            GameVersion::DestinyRiseOfIron => match package_manager().platform {
                PackagePlatform::PS4 => {
                    let texture: TextureHeaderRoiPs4 = package_manager().read_tag_binrw(hash)?;
                    Ok(TextureDesc {
                        format: texture.format.to_wgpu()?,
                        width: texture.width as u32,
                        height: texture.height as u32,
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
                        depth: texture.depth as u32,
                        premultiply_alpha: false,
                    })
                }
                _ => unreachable!("Unsupported platform for RoI textures"),
            },
            GameVersion::Destiny2Beta
            | GameVersion::Destiny2Forsaken
            | GameVersion::Destiny2Shadowkeep
            | GameVersion::Destiny2BeyondLight
            | GameVersion::Destiny2WitchQueen
            | GameVersion::Destiny2Lightfall
            | GameVersion::Destiny2TheFinalShape => {
                let header_data = package_manager()
                    .read_tag(hash)
                    .context("Failed to read texture header")?;

                let is_prebl = matches!(
                    package_manager().version,
                    destiny_pkg::GameVersion::Destiny2Beta
                        | destiny_pkg::GameVersion::Destiny2Shadowkeep
                );

                let mut cur = std::io::Cursor::new(header_data);
                let texture: TextureHeader = cur.read_le_args((is_prebl,))?;

                Ok(TextureDesc {
                    format: texture.format.to_wgpu()?,
                    width: texture.width as u32,
                    height: texture.height as u32,
                    depth: texture.depth as u32,
                    premultiply_alpha: false,
                })
            }
        }
    }

    pub fn load(
        rs: &RenderState,
        hash: TagHash,
        premultiply_alpha: bool,
    ) -> anyhow::Result<Texture> {
        if package_manager().version.is_d1()
            && !matches!(
                package_manager().platform,
                PackagePlatform::PS4 | PackagePlatform::XboxOne
            )
        {
            anyhow::bail!("Textures are not supported for D1");
        }

        match package_manager().version {
            GameVersion::DestinyInternalAlpha | GameVersion::DestinyTheTakenKing => todo!(),
            GameVersion::DestinyRiseOfIron => match package_manager().platform {
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
                            premultiply_alpha,
                        },
                        texture_data,
                        Some(comment),
                    )
                }
                PackagePlatform::XboxOne => {
                    anyhow::bail!("Xbox One textures are not supported yet");
                    let (texture, texture_data, comment) = Self::load_data_roi_xone(hash, true)?;
                    Self::create_texture(
                        rs,
                        hash,
                        TextureDesc {
                            format: texture.format.to_wgpu()?,
                            width: texture.width as u32,
                            height: texture.height as u32,
                            depth: texture.depth as u32,
                            premultiply_alpha,
                        },
                        texture_data,
                        Some(comment),
                    )
                }
                _ => unreachable!("Unsupported platform for RoI textures"),
            },
            GameVersion::Destiny2Beta
            | GameVersion::Destiny2Forsaken
            | GameVersion::Destiny2Shadowkeep
            | GameVersion::Destiny2BeyondLight
            | GameVersion::Destiny2WitchQueen
            | GameVersion::Destiny2Lightfall
            | GameVersion::Destiny2TheFinalShape => {
                let (texture, texture_data, comment) = Self::load_data_d2(hash, true)?;
                Self::create_texture(
                    rs,
                    hash,
                    TextureDesc {
                        format: texture.format.to_wgpu()?,
                        width: texture.width as u32,
                        height: texture.height as u32,
                        depth: texture.depth as u32,
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
        data: Vec<u8>,
        comment: Option<String>,
    ) -> anyhow::Result<Texture> {
        let mut texture_data = data;
        // Pre-multiply alpha where possible
        if matches!(
            desc.format,
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb
        ) {
            for c in texture_data.chunks_exact_mut(4) {
                c[0] = (c[0] as f32 * c[3] as f32 / 255.) as u8;
                c[1] = (c[1] as f32 * c[3] as f32 / 255.) as u8;
                c[2] = (c[2] as f32 * c[3] as f32 / 255.) as u8;
            }
        }

        let handle = rs.device.create_texture_with_data(
            &rs.queue,
            &wgpu::TextureDescriptor {
                label: Some(&*format!("Texture {hash}")),
                size: wgpu::Extent3d {
                    width: desc.width as _,
                    height: desc.height as _,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: desc.format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[desc.format],
            },
            wgpu::util::TextureDataOrder::default(),
            &texture_data,
        );

        let view = handle.create_view(&wgpu::TextureViewDescriptor {
            label: None,
            format: None,
            dimension: None,
            aspect: Default::default(),
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });

        Ok(Texture {
            view,
            handle,
            format: desc.format,
            aspect_ratio: desc.width as f32 / desc.height as f32,
            width: desc.width,
            height: desc.height,
            depth: desc.depth,
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
                depth: 1,
                premultiply_alpha: true,
            },
            rgba.into_raw(),
            None,
        )
    }

    // fn to_rgba(&self, rs: &RenderState) -> anyhow::Result<Vec<u8>> {
    //     capture_texture(rs, self)
    //         .map(|(data, _, _)| data)
    //         .context("Failed to capture texture")
    // }

    pub fn to_image(&self, rs: &RenderState) -> anyhow::Result<DynamicImage> {
        let (rgba_data, padded_width, padded_height) = capture_texture(rs, self)?;
        let image = image::RgbaImage::from_raw(padded_width, padded_height, rgba_data)
            .context("Failed to create image")?;

        Ok(DynamicImage::from(image).crop(0, 0, self.width, self.height))
    }
}

pub type LoadedTexture = (Arc<Texture>, TextureId);

type TextureCacheMap = LinkedHashMap<
    TagHash,
    Either<Option<LoadedTexture>, Promise<Option<LoadedTexture>>>,
    BuildHasherDefault<FxHasher>,
>;

#[derive(Clone)]
pub struct TextureCache {
    pub render_state: RenderState,
    cache: Rc<RwLock<TextureCacheMap>>,
    loading_placeholder: LoadedTexture,
}

impl TextureCache {
    pub fn new(render_state: RenderState) -> Self {
        let loading_placeholder =
            Texture::load_png(&render_state, include_bytes!("../../loading.png")).unwrap();

        let loading_placeholder_id = render_state.renderer.write().register_native_texture(
            &render_state.device,
            &loading_placeholder.view,
            wgpu::FilterMode::Linear,
        );

        Self {
            render_state,
            cache: Rc::new(RwLock::new(TextureCacheMap::default())),
            loading_placeholder: (Arc::new(loading_placeholder), loading_placeholder_id),
        }
    }

    pub fn is_loading_textures(&self) -> bool {
        self.cache
            .read()
            .iter()
            .any(|(_, v)| matches!(v, Either::Right(_)))
    }

    pub fn get_or_default(&self, hash: TagHash) -> LoadedTexture {
        self.get_or_load(hash)
            .unwrap_or_else(|| self.loading_placeholder.clone())
    }

    pub fn get_or_load(&self, hash: TagHash) -> Option<LoadedTexture> {
        let mut cache = self.cache.write();

        let c = cache.remove(&hash);

        let texture = if let Some(Either::Left(r)) = c {
            cache.insert(hash, Left(r.clone()));
            r.clone()
        } else if let Some(Either::Right(p)) = c {
            if let std::task::Poll::Ready(r) = p.poll() {
                cache.insert(hash, Left(r.clone()));
                return r.clone();
            } else {
                cache.insert(hash, Either::Right(p));
                None
            }
        } else if c.is_none() {
            cache.insert(
                hash,
                Either::Right(Promise::spawn_async(Self::load_texture_task(
                    self.render_state.clone(),
                    hash,
                ))),
            );

            None
        } else {
            None
        };

        drop(cache);
        self.truncate();

        texture
    }

    async fn load_texture_task(render_state: RenderState, hash: TagHash) -> Option<LoadedTexture> {
        let texture = match Texture::load(&render_state, hash, true) {
            Ok(t) => t,
            Err(e) => {
                log::error!("Failed to load texture {hash}: {e}");
                return None;
            }
        };

        let id = render_state.renderer.write().register_native_texture(
            &render_state.device,
            &texture.view,
            wgpu::FilterMode::Linear,
        );
        Some((Arc::new(texture), id))
    }

    pub fn texture_preview(&self, hash: TagHash, ui: &mut eframe::egui::Ui) {
        if let Some((tex, egui_tex)) = self.get_or_load(hash) {
            let screen_size = ui.ctx().screen_rect().size();
            let screen_aspect_ratio = screen_size.x / screen_size.y;
            let texture_aspect_ratio = tex.aspect_ratio;

            let max_size = if ui.input(|i| i.modifiers.ctrl) {
                screen_size * 0.70
            } else {
                ui.label("ℹ Hold ctrl to enlarge");
                screen_size * 0.30
            };

            let tex_size = if texture_aspect_ratio > screen_aspect_ratio {
                vec2(max_size.x, max_size.x / texture_aspect_ratio)
            } else {
                vec2(max_size.y * texture_aspect_ratio, max_size.y)
            };

            ui.image(SizedTexture::new(egui_tex, tex_size));

            ui.label(format!(
                "{}x{}x{} {:?}",
                tex.width, tex.height, tex.depth, tex.format
            ));
        }
    }
}

impl TextureCache {
    const MAX_TEXTURES: usize = 2048;
    fn truncate(&self) {
        let mut cache = self.cache.write();
        while cache.len() > Self::MAX_TEXTURES {
            if let Some((_, Either::Left(Some((_, tid))))) = cache.pop_front() {
                self.render_state.renderer.write().free_texture(&tid);
            }
        }
    }
}

mod swizzle {
    // https://github.com/tge-was-taken/GFD-Studio/blob/dad6c2183a6ec0716c3943b71991733bfbd4649d/GFDLibrary/Textures/Swizzle/SwizzleUtilities.cs#L9
    fn morton(t: usize, sx: usize, sy: usize) -> usize {
        let mut num1 = 1;
        let mut num2 = 1;
        let mut num3 = t;
        let mut num4 = sx;
        let mut num5 = sy;
        let mut num6 = 0;
        let mut num7 = 0;

        while num4 > 1 || num5 > 1 {
            if num4 > 1 {
                num6 += num2 * (num3 & 1);
                num3 >>= 1;
                num2 *= 2;
                num4 >>= 1;
            }
            if num5 > 1 {
                num7 += num1 * (num3 & 1);
                num3 >>= 1;
                num1 *= 2;
                num5 >>= 1;
            }
        }

        num7 * sx + num6
    }

    pub(crate) mod ps4 {
        use crate::gui::dxgi::GcnSurfaceFormat;

        // https://github.com/tge-was-taken/GFD-Studio/blob/dad6c2183a6ec0716c3943b71991733bfbd4649d/GFDLibrary/Textures/Swizzle/PS4SwizzleAlgorithm.cs#L20
        fn do_swizzle(
            source: &[u8],
            destination: &mut [u8],
            width: usize,
            height: usize,
            format: GcnSurfaceFormat,
            unswizzle: bool,
        ) {
            let pixel_block_size = format.pixel_block_size();
            let block_size = format.block_size();

            let width_src = if format.is_compressed() {
                width.next_power_of_two()
            } else {
                width
            };
            let height_src = if format.is_compressed() {
                height.next_power_of_two()
            } else {
                height
            };

            let width_texels_dest = width / pixel_block_size;
            let height_texels_dest = height / pixel_block_size;

            let width_texels = width_src / pixel_block_size;
            let width_texels_aligned = (width_texels + 7) / 8;
            let height_texels = height_src / pixel_block_size;
            let height_texels_aligned = (height_texels + 7) / 8;
            let mut data_index = 0;

            for y in 0..height_texels_aligned {
                for x in 0..width_texels_aligned {
                    for t in 0..64 {
                        let pixel_index = super::morton(t, 8, 8);
                        let div = pixel_index / 8;
                        let rem = pixel_index % 8;
                        let x_offset = (x * 8) + rem;
                        let y_offset = (y * 8) + div;

                        if x_offset < width_texels_dest && y_offset < height_texels_dest {
                            let dest_pixel_index = y_offset * width_texels_dest + x_offset;
                            let dest_index = block_size * dest_pixel_index;
                            let (src, dst) = if unswizzle {
                                (data_index, dest_index)
                            } else {
                                (dest_index, data_index)
                            };

                            if (src + block_size) < source.len()
                                && (dst + block_size) < destination.len()
                            {
                                destination[dst..dst + block_size]
                                    .copy_from_slice(&source[src..src + block_size]);
                            }
                        }

                        data_index += block_size;
                    }
                }
            }
        }

        pub(crate) fn unswizzle(
            source: &[u8],
            destination: &mut Vec<u8>,
            width: usize,
            height: usize,
            format: GcnSurfaceFormat,
        ) {
            destination.resize(source.len(), 0);
            do_swizzle(source, destination, width, height, format, true);
        }
    }

    pub(crate) mod xbox {
        use crate::gui::dxgi::DxgiFormat;

        pub(crate) fn untile(
            source: &[u8],
            destination: &mut Vec<u8>,
            width: usize,
            height: usize,
            format: DxgiFormat,
        ) {
            destination.resize(source.len(), 0);
            let copy_size = format.to_wgpu().unwrap().block_copy_size(None).unwrap();
            destination[..].copy_from_slice(&source);
        }
    }
}

mod texture_capture {
    // fn capture_texture(
    //     rs: &super::RenderState,
    //     texture: &super::Texture,
    // ) -> anyhow::Result<Vec<u8>> {
    //     todo!()
    // }

    /// Capture a texture to a raw RGBA buffer
    pub fn capture_texture(
        rs: &super::RenderState,
        texture: &super::Texture,
    ) -> anyhow::Result<(Vec<u8>, u32, u32)> {
        use eframe::wgpu::*;

        let super::RenderState { device, queue, .. } = rs;

        let texture_wgpu = device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width: texture.width,
                height: texture.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[TextureFormat::Rgba8UnormSrgb],
        });

        let texture_view_wgpu = texture_wgpu.create_view(&TextureViewDescriptor {
            label: None,
            format: Some(TextureFormat::Rgba8UnormSrgb),
            dimension: Some(TextureViewDimension::D2),
            aspect: TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: None,
        });

        // Create a buffer to hold the result of copying the texture to CPU memory
        let padded_width = (256.0 * (texture.width as f32 / 256.0).ceil()) as u32;
        let padded_height = (256.0 * (texture.height as f32 / 256.0).ceil()) as u32;
        let buffer_size = (padded_width * padded_height * 4) as usize;
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Output Buffer"),
            size: buffer_size as BufferAddress,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Bind Group Layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Create a render pipeline to copy the texture to an RGBA8 texture
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&texture.view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&device.create_sampler(
                        &SamplerDescriptor {
                            label: Some("Sampler"),
                            address_mode_u: AddressMode::ClampToEdge,
                            address_mode_v: AddressMode::ClampToEdge,
                            address_mode_w: AddressMode::ClampToEdge,
                            mag_filter: FilterMode::Nearest,
                            min_filter: FilterMode::Nearest,
                            mipmap_filter: FilterMode::Nearest,
                            ..Default::default()
                        },
                    )),
                },
            ],
        });

        let copy_shader = device.create_shader_module(include_wgsl!("shaders/copy.wgsl"));

        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            multiview: None,
            vertex: VertexState {
                module: &copy_shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(FragmentState {
                module: &copy_shader,
                entry_point: "fs_main",
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8UnormSrgb,
                    blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: ColorWrites::all(),
                })],
                compilation_options: Default::default(),
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Cw,
                cull_mode: Some(Face::Back),
                polygon_mode: PolygonMode::Fill,
                conservative: false,
                unclipped_depth: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        });

        // Copy the original texture to the RGBA8 texture using the render pipeline
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &texture_view_wgpu,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            render_pass.set_pipeline(&render_pipeline);
            render_pass.set_bind_group(0, &bind_group, &[]);
            // Draw a full-screen quad to trigger the fragment shader
            render_pass.draw(0..3, 0..1);
        }

        // Submit commands
        queue.submit(Some(encoder.finish()));

        // Copy the texture data to the CPU-accessible buffer
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            encoder.copy_texture_to_buffer(
                ImageCopyTexture {
                    aspect: TextureAspect::All,
                    texture: &texture_wgpu,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                },
                ImageCopyBuffer {
                    buffer: &buffer,
                    layout: ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * padded_width),
                        rows_per_image: Some(padded_height),
                    },
                },
                Extent3d {
                    width: texture.width,
                    height: texture.height,
                    depth_or_array_layers: 1,
                },
            );
        }

        // Submit commands
        queue.submit(Some(encoder.finish()));

        // Wait for the copy operation to complete
        device.poll(Maintain::Wait);

        let buffer_slice = buffer.slice(..);
        buffer_slice.map_async(MapMode::Read, |_| {});
        device.poll(Maintain::Wait);
        let buffer_view = buffer_slice.get_mapped_range();
        let buffer_data = buffer_view.to_vec();
        // let final_size = (texture.width * texture.height * 4) as usize;
        // buffer_data.truncate(final_size);

        Ok((buffer_data, padded_width, padded_height))
    }
}
