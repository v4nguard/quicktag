use crate::gui::dxgi::{GcmSurfaceFormat, GcnSurfaceFormat};

use super::Deswizzler;

pub struct GcmDeswizzler;

impl GcmDeswizzler {
    pub fn color_deswizzle(source: &[u8], format: GcmSurfaceFormat) -> Vec<u8> {
        let mut result = Vec::with_capacity(source.len());
        match format {
            // ARGB => RGBA
            GcmSurfaceFormat::A8R8G8B8 => {
                for chunk in source.chunks_exact(4) {
                    result.extend_from_slice(&[chunk[1], chunk[2], chunk[3], chunk[0]]);
                }
                result
            }
            _ => source.to_vec(),
        }
    }
}

impl Deswizzler for GcmDeswizzler {
    type Format = GcmSurfaceFormat;
    fn deswizzle(
        &self,
        source: &[u8],
        width: usize,
        height: usize,
        depth: usize,
        format: Self::Format,
        align_resolution: bool,
    ) -> anyhow::Result<Vec<u8>> {
        Ok(ps3::do_swizzle(
            source,
            width,
            height,
            depth,
            format,
            true,
            align_resolution,
        ))
    }
}

mod ps3 {
    use crate::{gui::dxgi::GcmSurfaceFormat, texture::swizzle::morton};

    pub fn do_swizzle(
        source: &[u8],
        width: usize,
        height: usize,
        depth: usize,
        format: GcmSurfaceFormat,
        unswizzle: bool,
        align_resolution: bool,
    ) -> Vec<u8> {
        let mut untiled = vec![0; source.len()];
        let pixel_block_size = format.pixel_block_size();
        let block_size = format.block_size();

        let (width_src, height_src) = if align_resolution && format.is_compressed() {
            (width.next_power_of_two(), height.next_power_of_two())
        } else {
            (width, height)
        };

        let width_texels = width_src / pixel_block_size;
        let height_texels = height_src / pixel_block_size;

        let mut data_index = 0;

        let texel_size = width_texels * height_texels;

        for z in 0..depth {
            let slice_dest = &mut untiled[(z * width * height * format.bpp()) / 8..];

            for t in 0..texel_size {
                let pixel_index = morton(t, width_texels, height_texels);
                let dest_index = block_size * pixel_index;
                let (src, dst) = if unswizzle {
                    (data_index, dest_index)
                } else {
                    (dest_index, data_index)
                };

                if (src + block_size) <= source.len() && (dst + block_size) <= slice_dest.len() {
                    slice_dest[dst..dst + block_size]
                        .copy_from_slice(&source[src..src + block_size]);
                }

                data_index += block_size;
            }
        }
        untiled
    }
}

pub struct GcnDeswizzler;

impl Deswizzler for GcnDeswizzler {
    type Format = GcnSurfaceFormat;
    fn deswizzle(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        depth_or_array_size: usize,
        format: Self::Format,
        align_output: bool,
    ) -> anyhow::Result<Vec<u8>> {
        Ok(ps4::do_swizzle(
            data,
            width,
            height,
            depth_or_array_size,
            format,
            true,
            align_output,
        ))
    }
}

mod ps4 {
    use crate::{gui::dxgi::GcnSurfaceFormat, texture::swizzle::morton};

    pub fn do_swizzle(
        source: &[u8],
        width: usize,
        height: usize,
        depth: usize,
        format: GcnSurfaceFormat,
        unswizzle: bool,
        align_resolution: bool,
    ) -> Vec<u8> {
        let mut destination = vec![0; source.len()];
        let pixel_block_size = format.pixel_block_size();
        let block_size = format.block_size();

        let (width_src, height_src) = if align_resolution && format.is_compressed() {
            (width.next_power_of_two(), height.next_power_of_two())
        } else {
            (width, height)
        };

        let width_texels_dest = width / pixel_block_size;
        let height_texels_dest = height / pixel_block_size;

        let width_texels = width_src / pixel_block_size;
        let width_texels_aligned = (width_texels + 7) / 8;
        let height_texels = height_src / pixel_block_size;
        let height_texels_aligned = (height_texels + 7) / 8;
        let mut data_index = 0;

        for z in 0..depth {
            let slice_dest = &mut destination[(z * width * height * format.bpp()) / 8..];

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

                            if (src + block_size) <= source.len()
                                && (dst + block_size) <= slice_dest.len()
                            {
                                slice_dest[dst..dst + block_size]
                                    .copy_from_slice(&source[src..src + block_size]);
                            }
                        }

                        data_index += block_size;
                    }
                }
            }
        }
        destination
    }
}
