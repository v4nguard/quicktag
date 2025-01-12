use crate::gui::dxgi::GcnSurfaceFormat;

use super::Deswizzler;

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
                let pixel_index = morton(t, 8, 8);
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

                    if (src + block_size) < source.len() && (dst + block_size) < destination.len() {
                        destination[dst..dst + block_size]
                            .copy_from_slice(&source[src..src + block_size]);
                    }
                }

                data_index += block_size;
            }
        }
    }
}

pub struct GcnDeswizzler;

impl Deswizzler for GcnDeswizzler {
    type Format = GcnSurfaceFormat;
    fn deswizzle(
        data: &[u8],
        width: usize,
        height: usize,
        _depth: usize,
        format: Self::Format,
    ) -> anyhow::Result<Vec<u8>> {
        let mut converted_data = vec![0; data.len()];
        do_swizzle(data, &mut converted_data, width, height, format, true);
        Ok(converted_data)
    }
}
