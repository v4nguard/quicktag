pub mod swizzle_ps;
pub mod swizzle_xbox;

pub trait Deswizzler {
    type Format;
    fn deswizzle(
        &self,
        source: &[u8],
        width: usize,
        height: usize,
        depth: usize,
        format: Self::Format,
        align_resolution: bool,
    ) -> anyhow::Result<Vec<u8>>;
}

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
