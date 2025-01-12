// Adapted from https://github.com/bartlomiejduda/ReverseBox/blob/main/reversebox/image/swizzling/swizzle_x360.py

use anyhow::Context;
use log::warn;

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
        (((offset_byte & !0xFFF) >> 3) + ((offset_byte & 0x700) >> 2) + (offset_byte & 0x3F));
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
        (((offset_byte & !0xFFF) >> 3) + ((offset_byte & 0x700) >> 2) + (offset_byte & 0x3F));
    let offset_macro = offset_tile >> (7 + log_bpp);

    let macro_y = (offset_macro / (aligned_width >> 5)) << 2;
    let tile = ((offset_tile >> (6 + log_bpp)) & 1) + ((offset_byte & 0x800) >> 10);
    let macro_ = (macro_y + tile) << 3;
    let micro = (((offset_tile & ((texel_byte_pitch << 6) - 1 & !0x1F))
        + ((offset_tile & 0xF) << 1))
        >> (3 + log_bpp))
        & !1;

    macro_ + micro + ((offset_tile & 0x10) >> 4)
}

fn convert_x360_image_data(
    image_data: &[u8],
    image_width: usize,
    image_height: usize,
    block_pixel_size: usize,
    texel_byte_pitch: usize,
    swizzle_flag: bool,
) -> anyhow::Result<Vec<u8>> {
    let mut converted_data = vec![0; image_data.len()];

    let width_in_blocks = image_width / block_pixel_size;
    let height_in_blocks = image_height / block_pixel_size;

    for j in 0..height_in_blocks {
        for i in 0..width_in_blocks {
            let block_offset = j * width_in_blocks + i;
            let x = xg_address_2d_tiled_x(block_offset, width_in_blocks, texel_byte_pitch);
            let y = xg_address_2d_tiled_y(block_offset, width_in_blocks, texel_byte_pitch);
            let src_byte_offset = j * width_in_blocks * texel_byte_pitch + i * texel_byte_pitch;
            let dest_byte_offset = y * width_in_blocks * texel_byte_pitch + x * texel_byte_pitch;

            if dest_byte_offset + texel_byte_pitch > converted_data.len() {
                continue;
            }
            if !swizzle_flag {
                match image_data.get(src_byte_offset..src_byte_offset + texel_byte_pitch) {
                    Some(source) => {
                        converted_data[dest_byte_offset..dest_byte_offset + texel_byte_pitch]
                            .copy_from_slice(source);
                    }
                    None => {
                        continue;
                    }
                }
            } else {
                match image_data.get(dest_byte_offset..dest_byte_offset + texel_byte_pitch) {
                    Some(source) => {
                        converted_data[src_byte_offset..src_byte_offset + texel_byte_pitch]
                            .copy_from_slice(source);
                    }
                    None => {
                        continue;
                    }
                }
            }
        }
    }

    Ok(converted_data)
}

pub fn untile_x360(
    image_data: &[u8],
    img_width: usize,
    img_height: usize,
    block_pixel_size: usize,
    texel_byte_pitch: usize,
) -> anyhow::Result<Vec<u8>> {
    // let swapped_data = swap_byte_order(image_data);
    convert_x360_image_data(
        image_data,
        img_width,
        img_height,
        block_pixel_size,
        texel_byte_pitch,
        false,
    )
}
