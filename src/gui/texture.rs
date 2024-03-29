use crate::gui::dxgi::DxgiFormat;
use crate::packages::package_manager;
use anyhow::Context;
use binrw::BinRead;
use destiny_pkg::package::PackagePlatform;
use destiny_pkg::TagHash;
use eframe::egui::load::SizedTexture;
use eframe::egui_wgpu::RenderState;
use eframe::epaint::mutex::RwLock;
use eframe::epaint::{vec2, Color32, TextureId};
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use eframe::wgpu::TextureDimension;
use linked_hash_map::LinkedHashMap;
use rustc_hash::FxHasher;
use std::hash::BuildHasherDefault;
use std::io::SeekFrom;
use std::sync::Arc;

use super::dxgi::GcnSurfaceFormat;

#[derive(BinRead)]
pub struct TextureHeader {
    pub data_size: u32,
    pub format: DxgiFormat,
    pub _unk8: u32,

    #[br(seek_before = SeekFrom::Start(0x20), assert(cafe == 0xcafe))]
    pub cafe: u16,

    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub array_size: u16,

    pub unk2a: u16,
    pub unk2c: u8,
    pub mip_count: u8,
    pub unk2e: [u8; 10],
    pub unk38: u32,

    #[br(seek_before = SeekFrom::Start(0x3c))]
    #[br(map(|v: u32| (v != u32::MAX).then_some(TagHash(v))))]
    pub large_buffer: Option<TagHash>,
}

#[derive(BinRead)]
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

    pub unk30: u32,
    pub flags: u8,
}

pub struct Texture {
    pub view: wgpu::TextureView,
    pub handle: wgpu::Texture,
    pub format: wgpu::TextureFormat,
    pub aspect_ratio: f32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

impl Texture {
    pub fn load_data(
        hash: TagHash,
        load_full_mip: bool,
    ) -> anyhow::Result<(TextureHeader, Vec<u8>)> {
        let texture_header_ref = package_manager()
            .get_entry(hash)
            .context("Texture header entry not found")?
            .reference;

        let texture: TextureHeader = package_manager().read_tag_binrw(hash)?;
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

        Ok((texture, texture_data))
    }

    pub fn load_data_roi_ps4(
        hash: TagHash,
        _load_full_mip: bool,
    ) -> anyhow::Result<(TextureHeaderRoiPs4, Vec<u8>)> {
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

        if texture.flags > 0 || !texture.format.is_compressed() {
            let mut unswizzled = vec![];
            swizzle::ps4::unswizzle(
                &texture_data,
                &mut unswizzled,
                texture.width as usize,
                texture.height as usize,
                texture.format.block_size(),
                texture.format.pixel_block_size(),
            );
            Ok((texture, unswizzled))
        } else {
            Ok((texture, texture_data))
        }
    }

    pub fn load(rs: &RenderState, hash: TagHash) -> anyhow::Result<Texture> {
        if package_manager().version.is_d1() && package_manager().platform != PackagePlatform::PS4 {
            anyhow::bail!("Textures are not supported for D1");
        }

        match package_manager().version {
            destiny_pkg::PackageVersion::DestinyTheTakenKing => todo!(),
            destiny_pkg::PackageVersion::DestinyRiseOfIron => {
                let (texture, texture_data) = Self::load_data_roi_ps4(hash, true)?;
                Self::create_texture(
                    rs,
                    hash,
                    texture.format.to_wgpu()?,
                    texture.width as u32,
                    texture.height as u32,
                    texture.depth as u32,
                    texture_data,
                )
            }
            destiny_pkg::PackageVersion::Destiny2Beta
            | destiny_pkg::PackageVersion::Destiny2Shadowkeep
            | destiny_pkg::PackageVersion::Destiny2BeyondLight
            | destiny_pkg::PackageVersion::Destiny2WitchQueen
            | destiny_pkg::PackageVersion::Destiny2Lightfall => {
                let (texture, texture_data) = Self::load_data(hash, true)?;
                Self::create_texture(
                    rs,
                    hash,
                    texture.format.to_wgpu()?,
                    texture.width as u32,
                    texture.height as u32,
                    texture.depth as u32,
                    texture_data,
                )
            }
        }
    }

    /// Create a wgpu texture from unswizzled texture data
    fn create_texture(
        rs: &RenderState,
        hash: TagHash,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        depth: u32,
        // cohae: Take ownership of the data so we don't have to clone it for premultiplication
        data: Vec<u8>,
    ) -> anyhow::Result<Texture> {
        let mut texture_data = data;
        // Pre-multiply alpha where possible
        if matches!(
            format,
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
                    width: width as _,
                    height: height as _,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[format],
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
            format,
            aspect_ratio: width as f32 / height as f32,
            width,
            height,
            depth,
        })
    }
}

type TextureCacheMap =
    LinkedHashMap<TagHash, Option<(Arc<Texture>, TextureId)>, BuildHasherDefault<FxHasher>>;

#[derive(Clone)]
pub struct TextureCache {
    render_state: RenderState,
    cache: Arc<RwLock<TextureCacheMap>>,
}

impl TextureCache {
    pub fn new(render_state: RenderState) -> Self {
        Self {
            render_state,
            cache: Arc::new(RwLock::new(TextureCacheMap::default())),
        }
    }

    pub fn get_or_load(&self, hash: TagHash) -> Option<(Arc<Texture>, TextureId)> {
        let mut cache = self.cache.write();
        if let Some(t) = cache.get(&hash) {
            return t.clone();
        }

        let texture = match Texture::load(&self.render_state, hash) {
            Ok(o) => o,
            Err(e) => {
                log::error!("Failed to load texture {hash}: {e}");
                cache.insert(hash, None);
                return None;
            }
        };
        let id = self.render_state.renderer.write().register_native_texture(
            &self.render_state.device,
            &texture.view,
            wgpu::FilterMode::Linear,
        );

        cache.insert(hash, Some((Arc::new(texture), id)));
        drop(cache);

        self.truncate();
        self.cache.read().get(&hash).cloned().unwrap()
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
        } else {
            ui.colored_label(
                Color32::RED,
                "⚠ Texture not found, check log for more information",
            );
        }
    }
}

impl TextureCache {
    const MAX_TEXTURES: usize = 64;
    fn truncate(&self) {
        let mut cache = self.cache.write();
        while cache.len() > Self::MAX_TEXTURES {
            if let Some((_, Some((_, tid)))) = cache.pop_front() {
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
        // https://github.com/tge-was-taken/GFD-Studio/blob/dad6c2183a6ec0716c3943b71991733bfbd4649d/GFDLibrary/Textures/Swizzle/PS4SwizzleAlgorithm.cs#L20
        fn do_swizzle(
            source: &[u8],
            destination: &mut Vec<u8>,
            width: usize,
            height: usize,
            block_size: usize,
            pixel_block_size: usize,
            unswizzle: bool,
        ) {
            destination.resize(source.len(), 0);
            let width_texels = width / pixel_block_size;
            let width_texels_aligned = (width_texels + 7) / 8;
            let height_texels = height / pixel_block_size;
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

                        if x_offset < width_texels && y_offset < height_texels {
                            let dest_pixel_index = y_offset * width_texels + x_offset;
                            let dest_index = block_size * dest_pixel_index;
                            let (src, dst) = if unswizzle {
                                (data_index, dest_index)
                            } else {
                                (dest_index, data_index)
                            };

                            destination[dst..dst + block_size]
                                .copy_from_slice(&source[src..src + block_size]);
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
            block_size: usize,
            pixel_block_size: usize,
        ) {
            do_swizzle(
                source,
                destination,
                width,
                height,
                block_size,
                pixel_block_size,
                true,
            );
        }
    }
}
