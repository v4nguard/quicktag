use std::mem::MaybeUninit;

use crate::structs::{XgResourceLayout, XgTexture2DDesc};

pub mod structs;

#[link(name = "xg", kind = "raw-dylib")]
unsafe extern "C" {
    fn XGCreateTexture2DComputer(
        desc: *const XgTexture2DDesc,
        out: *mut *mut std::ffi::c_void,
    ) -> i32;
}

pub struct XgTexture2DComputer {
    ptr: *mut std::ffi::c_void,
}

impl XgTexture2DComputer {
    pub fn new(desc: &XgTexture2DDesc) -> Result<Self, i32> {
        let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let hr = unsafe { XGCreateTexture2DComputer(desc, &mut ptr) };
        if hr < 0 {
            return Err(hr);
        }
        Ok(Self { ptr })
    }

    pub fn get_resource_layout(&self) -> Result<XgResourceLayout, i32> {
        let mut layout = MaybeUninit::<XgResourceLayout>::uninit();
        let hr = (self.vt().get_resource_layout)(self.ptr, layout.as_mut_ptr());
        if hr < 0 {
            return Err(hr);
        }
        Ok(unsafe { layout.assume_init() })
    }

    pub fn get_texel_element_offset_bytes(
        &self,
        subresource: u32,
        level: u32,
        x: u64,
        y: u32,
        array_index: u32,
        element: u32,
    ) -> i64 {
        (self.vt().get_texel_element_offset_bytes)(
            self.ptr,
            subresource,
            level,
            x,
            y,
            array_index,
            element,
        )
    }

    fn vt(&self) -> &XgTextureComputerVtable {
        unsafe { &*(self.ptr as *const *const XgTextureComputerVtable).read() }
    }
}

// https://github.com/Gravemind2401/Reclaimer/blob/master/Reclaimer.Blam/Utilities/XG.cs#L284
#[repr(C)]
struct XgTextureComputerVtable {
    add_ref: extern "C" fn(*mut std::ffi::c_void) -> u32,
    release: extern "C" fn(*mut std::ffi::c_void) -> u32,
    get_resource_layout: extern "C" fn(*mut std::ffi::c_void, *mut XgResourceLayout) -> i32,
    get_resource_size_bytes: extern "C" fn(*mut std::ffi::c_void) -> u64,
    get_resource_base_alignment_bytes: extern "C" fn(*mut std::ffi::c_void) -> u64,
    get_mip_level_offset_bytes: extern "C" fn(*mut std::ffi::c_void, u32, u32) -> u64,
    get_texel_element_offset_bytes:
        extern "C" fn(*mut std::ffi::c_void, u32, u32, u64, u32, u32, u32) -> i64,
    get_texel_coordinate: extern "C" fn(
        *mut std::ffi::c_void,
        u64,
        *mut u32,
        *mut u32,
        *mut u64,
        *mut u32,
        *mut u32,
        *mut u32,
    ) -> i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_desc() -> XgTexture2DDesc {
        XgTexture2DDesc {
            width: 256,
            height: 256,
            mip_levels: 9,
            array_size: 1,
            format: 29, // DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
            sample_desc: crate::structs::XgSampleDesc {
                count: 1,
                quality: 0,
            },
            usage: 0,      // D3D11_USAGE_DEFAULT
            bind_flags: 8, // D3D11_BIND_SHADER_RESOURCE
            cpu_access_flags: 0,
            misc_flags: 0,
            esram_offset_bytes: 0,
            esram_usage_bytes: 0,
            tile_mode: 14, // XG_TILE_MODE_2D_THIN
            pitch: 0,
        }
    }

    #[test]
    fn test_create_computer() {
        let desc = make_desc();
        let computer = XgTexture2DComputer::new(&desc).expect("Failed to create computer");
        let layout = computer
            .get_resource_layout()
            .expect("Failed to get resource layout");
        println!("{:#?}", layout);
    }
}
