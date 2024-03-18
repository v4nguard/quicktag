use crate::gui::dxgi::DxgiFormat;
use crate::packages::package_manager;
use anyhow::Context;
use binrw::BinRead;
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

pub struct Texture {
    pub view: wgpu::TextureView,
    pub handle: wgpu::Texture,
    pub format: DxgiFormat,
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

    pub fn load(rs: &RenderState, hash: TagHash) -> anyhow::Result<Texture> {
        if package_manager().version.is_d1() {
            anyhow::bail!("Textures are not supported for D1");
        }

        let (texture, mut texture_data) = Self::load_data(hash, true)?;
        // Pre-multiply alpha where possible
        if matches!(
            texture.format,
            DxgiFormat::R8G8B8A8_UNORM_SRGB | DxgiFormat::R8G8B8A8_UNORM
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
                    width: texture.width as _,
                    height: texture.height as _,
                    depth_or_array_layers: 1,
                    // depth_or_array_layers: if texture.depth == 1 {
                    //     // texture.array_size as _
                    //     1
                    // } else {
                    //     // texture.depth as _
                    //     1
                    // },
                },
                mip_level_count: texture.mip_count as u32,
                sample_count: 1,
                dimension: TextureDimension::D2,
                // dimension: if texture.depth == 1 {
                //     TextureDimension::D2
                // } else {
                //     TextureDimension::D3
                // },
                format: texture.format.to_wgpu()?,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[texture.format.to_wgpu()?],
            },
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
            format: texture.format,
            aspect_ratio: texture.width as f32 / texture.height as f32,
            width: texture.width as u32,
            height: texture.height as u32,
            depth: texture.depth as u32,
        })
    }
}

type TextureCacheMap =
    LinkedHashMap<TagHash, (Arc<Texture>, TextureId), BuildHasherDefault<FxHasher>>;

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

    pub fn get_or_load(&self, hash: TagHash) -> anyhow::Result<(Arc<Texture>, TextureId)> {
        let mut cache = self.cache.write();
        if let Some(t) = cache.get(&hash) {
            return Ok(t.clone());
        }

        let texture = Texture::load(&self.render_state, hash)?;
        let id = self.render_state.renderer.write().register_native_texture(
            &self.render_state.device,
            &texture.view,
            wgpu::FilterMode::Linear,
        );

        cache.insert(hash, (Arc::new(texture), id));
        drop(cache);

        self.truncate();
        Ok(self.cache.read().get(&hash).cloned().unwrap())
    }

    pub fn texture_preview(&self, hash: TagHash, ui: &mut eframe::egui::Ui) {
        if let Ok((tex, egui_tex)) = self.get_or_load(hash) {
            let max_height = ui.available_height() * 0.90;

            let tex_size = if ui.input(|i| i.modifiers.ctrl) {
                vec2(max_height * tex.aspect_ratio, max_height)
            } else {
                ui.label("ℹ Hold ctrl to enlarge");
                vec2(256. * tex.aspect_ratio, 256.)
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
            if let Some((_, (_, tid))) = cache.pop_front() {
                self.render_state.renderer.write().free_texture(&tid);
            }
        }
    }
}
