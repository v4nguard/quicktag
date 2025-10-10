// Adapted from https://github.com/bartlomiejduda/ReverseBox/blob/main/reversebox/image/swizzling/swizzle_x360.py

use anyhow::Context;
use log::warn;
use xg::structs::XgTexture2DDesc;

use crate::texture::dxgi::{DxgiFormat, XenosSurfaceFormat};

use super::Deswizzler;

pub fn swap_byte_order_x360(image_data: &mut [u8]) {
    for chunk in image_data.chunks_mut(2) {
        chunk.swap(0, 1);
    }
}

fn xg_address_2d_tiled_x(
    block_offset: usize,
    width_in_blocks: usize,
    texel_byte_pitch: usize,
) -> usize {
    let aligned_width = (width_in_blocks + 31) & !31;
    let log_bpp = (texel_byte_pitch >> 2) + ((texel_byte_pitch >> 1) >> (texel_byte_pitch >> 2));
    let offset_byte = block_offset << log_bpp;
    let offset_tile =
        ((offset_byte & !0xFFF) >> 3) + ((offset_byte & 0x700) >> 2) + (offset_byte & 0x3F);
    let offset_macro = offset_tile >> (7 + log_bpp);

    let macro_x = (offset_macro % (aligned_width >> 5)) << 2;
    let tile = (((offset_tile >> (5 + log_bpp)) & 2) + (offset_byte >> 6)) & 3;
    let macro_ = (macro_x + tile) << 3;
    let micro = ((((offset_tile >> 1) & !0xF) + (offset_tile & 0xF))
        & ((texel_byte_pitch << 3) - 1))
        >> log_bpp;

    macro_ + micro
}

fn xg_address_2d_tiled_y(
    block_offset: usize,
    width_in_blocks: usize,
    texel_byte_pitch: usize,
) -> usize {
    let aligned_width = (width_in_blocks + 31) & !31;
    let log_bpp = (texel_byte_pitch >> 2) + ((texel_byte_pitch >> 1) >> (texel_byte_pitch >> 2));
    let offset_byte = block_offset << log_bpp;
    let offset_tile =
        ((offset_byte & !0xFFF) >> 3) + ((offset_byte & 0x700) >> 2) + (offset_byte & 0x3F);
    let offset_macro = offset_tile >> (7 + log_bpp);

    let macro_y = (offset_macro / (aligned_width >> 5)) << 2;
    let tile = ((offset_tile >> (6 + log_bpp)) & 1) + ((offset_byte & 0x800) >> 10);
    let macro_ = (macro_y + tile) << 3;
    let micro = (((offset_tile & (((texel_byte_pitch << 6) - 1) & !0x1F))
        + ((offset_tile & 0xF) << 1))
        >> (3 + log_bpp))
        & !1;

    macro_ + micro + ((offset_tile & 0x10) >> 4)
}

fn untile_x360_image_data(
    image_data: &[u8],
    image_width: usize,
    image_height: usize,
    image_depth: usize,
    block_pixel_size: usize,
    texel_byte_pitch: usize,
    deswizzle: bool,
) -> anyhow::Result<Vec<u8>> {
    let mut converted_data = vec![0; image_data.len()];

    let width_in_blocks = image_width / block_pixel_size;
    let height_in_blocks = image_height / block_pixel_size;

    let padded_width_in_blocks = (width_in_blocks + 31) & !31;
    let padded_height_in_blocks = (height_in_blocks + 31) & !31;

    let slice_size = padded_width_in_blocks * padded_height_in_blocks * texel_byte_pitch;

    for slice in 0..image_depth {
        let slice_src = image_data
            .get(slice * slice_size..)
            .context("Texture slice source out of bounds")?;
        let slice_dest = converted_data
            .get_mut(slice * slice_size..)
            .context("Texture slice dest out of bounds")?;

        for j in 0..padded_height_in_blocks {
            for i in 0..padded_width_in_blocks {
                let block_offset = j * padded_width_in_blocks + i;
                let x =
                    xg_address_2d_tiled_x(block_offset, padded_width_in_blocks, texel_byte_pitch);
                let y =
                    xg_address_2d_tiled_y(block_offset, padded_width_in_blocks, texel_byte_pitch);
                let src_byte_offset = block_offset * texel_byte_pitch;
                let dest_byte_offset = (y * width_in_blocks + x) * texel_byte_pitch;

                if dest_byte_offset + texel_byte_pitch > slice_dest.len()
                    || src_byte_offset + texel_byte_pitch > slice_src.len()
                {
                    continue;
                }

                if deswizzle {
                    match slice_src.get(src_byte_offset..src_byte_offset + texel_byte_pitch) {
                        Some(source) => {
                            if source.iter().all(|&b| b == 0) {
                                continue;
                            }
                            slice_dest[dest_byte_offset..dest_byte_offset + texel_byte_pitch]
                                .copy_from_slice(source);
                        }
                        None => {
                            continue;
                        }
                    }
                } else {
                    match slice_src.get(dest_byte_offset..dest_byte_offset + texel_byte_pitch) {
                        Some(source) => {
                            slice_dest[src_byte_offset..src_byte_offset + texel_byte_pitch]
                                .copy_from_slice(source);
                        }
                        None => {
                            continue;
                        }
                    }
                }
            }
        }
    }

    Ok(converted_data)
}

pub struct XenosDetiler;

impl Deswizzler for XenosDetiler {
    type Format = XenosSurfaceFormat;
    fn deswizzle(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        depth_or_array_size: usize,
        format: Self::Format,
        _align_output: bool,
    ) -> anyhow::Result<Vec<u8>> {
        let block_pixel_size;
        let texel_byte_pitch;
        match format {
            XenosSurfaceFormat::k_DXT1 | XenosSurfaceFormat::k_DXT1_AS_16_16_16_16 => {
                block_pixel_size = 4;
                texel_byte_pitch = 8;
            }
            XenosSurfaceFormat::k_DXN
            | XenosSurfaceFormat::k_DXT2_3
            | XenosSurfaceFormat::k_DXT2_3_AS_16_16_16_16
            | XenosSurfaceFormat::k_DXT3A
            | XenosSurfaceFormat::k_DXT3A_AS_1_1_1_1
            | XenosSurfaceFormat::k_DXT4_5
            | XenosSurfaceFormat::k_DXT4_5_AS_16_16_16_16
            | XenosSurfaceFormat::k_DXT5A => {
                block_pixel_size = 4;
                texel_byte_pitch = 16;
            }
            XenosSurfaceFormat::k_8_8_8_8
            | XenosSurfaceFormat::k_8_8_8_8_A
            | XenosSurfaceFormat::k_8_8_8_8_AS_16_16_16_16 => {
                block_pixel_size = 1;
                texel_byte_pitch = 4;
            }
            XenosSurfaceFormat::k_8 => {
                block_pixel_size = 1;
                texel_byte_pitch = 1;
            }
            _ => {
                warn!("Unsupported format for untile: {:?}", format);
                block_pixel_size = 1;
                texel_byte_pitch = 4;
            }
        };

        let mut source = data.to_vec();
        if matches!(
            format,
            XenosSurfaceFormat::k_DXT1
                | XenosSurfaceFormat::k_DXT1_AS_16_16_16_16
                | XenosSurfaceFormat::k_DXN
                | XenosSurfaceFormat::k_DXT2_3
                | XenosSurfaceFormat::k_DXT2_3_AS_16_16_16_16
                | XenosSurfaceFormat::k_DXT3A
                | XenosSurfaceFormat::k_DXT3A_AS_1_1_1_1
                | XenosSurfaceFormat::k_DXT4_5
                | XenosSurfaceFormat::k_DXT4_5_AS_16_16_16_16
                | XenosSurfaceFormat::k_DXT5A
        ) {
            swap_byte_order_x360(&mut source);
        }

        let untiled = untile_x360_image_data(
            &source,
            width,
            height,
            depth_or_array_size,
            block_pixel_size,
            texel_byte_pitch,
            true,
        )?;

        let mut result = Vec::with_capacity(untiled.len());
        match format {
            // ARGB => RGBA
            XenosSurfaceFormat::k_8_8_8_8 | XenosSurfaceFormat::k_8_8_8_8_A => {
                for chunk in untiled.chunks_exact(4) {
                    result.extend_from_slice(&[chunk[1], chunk[2], chunk[3], chunk[0]]);
                }
            }
            _ => {
                result.extend_from_slice(&untiled);
            }
        }

        Ok(result)
    }
}

pub struct DurangoDeswizzler;

impl Deswizzler for DurangoDeswizzler {
    type Format = (DxgiFormat, u32);
    fn deswizzle(
        &self,
        source: &[u8],
        width: usize,
        height: usize,
        depth_or_array_size: usize,
        (format, tile_mode): Self::Format,
        _align_output: bool,
    ) -> anyhow::Result<Vec<u8>> {
        if depth_or_array_size > 1 {
            warn!("Array/3D textures are not supported yet.");
            return Ok(source.to_vec());
        }

        let mut output = vec![0; source.len()];

        let comp = match xg::XgTexture2DComputer::new(&XgTexture2DDesc {
            width: width as u32,
            height: height as u32,
            mip_levels: 1,
            array_size: depth_or_array_size as u32,
            format: format as u32,
            sample_desc: xg::structs::XgSampleDesc {
                count: 1,
                quality: 0,
            },
            usage: 0,
            bind_flags: 8,
            cpu_access_flags: 0,
            misc_flags: 0,
            esram_offset_bytes: 0,
            esram_usage_bytes: 0,
            tile_mode,
            pitch: 0,
        }) {
            Ok(o) => o,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to create texture computer: {e:08X}"
                ));
            }
        };

        let layout = match comp.get_resource_layout() {
            Ok(l) => l,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to get resource layout: {e:08X}"));
            }
        };

        let mip = &layout.planes[0].mips[0];
        let blocks_x = mip.width_elements as usize;
        let blocks_y = mip.height_elements as usize;
        let texel_pitch = (mip.pitch_bytes / mip.padded_width_elements) as usize;

        for y in 0..blocks_y {
            for x in 0..blocks_x {
                let src_offset =
                    comp.get_texel_element_offset_bytes(0, 0, x as u64, y as u32, 0, 0);
                let dst_offset = y * blocks_x * texel_pitch + x * texel_pitch;

                if src_offset < 0 || (src_offset as usize) + texel_pitch > source.len() {
                    continue;
                }
                let src_offset = src_offset as usize;

                output[dst_offset..dst_offset + texel_pitch as usize]
                    .copy_from_slice(&source[src_offset..src_offset + texel_pitch as usize]);
            }
        }

        Ok(output)
    }
}
