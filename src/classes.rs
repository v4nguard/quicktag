use std::{
    borrow::Cow,
    fmt::{Debug, Display, UpperHex},
    sync::{atomic::AtomicBool, Arc},
};

use anyhow::Context;
use arc_swap::ArcSwap;
use binrw::Endian;
use bytemuck::{Pod, Zeroable};
use rustc_hash::FxHashMap;
use tiger_pkg::{DestinyVersion, GameVersion, TagHash};

use crate::{
    package_manager::{package_manager, package_manager_checked},
    util::u32_from_endian,
};

#[derive(Clone)]
pub struct TagClass {
    pub id: u32,
    pub name: Cow<'static, str>,
    pub size: Option<usize>,
    pub pretty_parser: Option<fn(&[u8], Endian) -> String>,
    /// Should this type block tag scanning? Useful for eliminating false positives in byte/vec4 blobs
    pub block_tags: bool,
}

impl TagClass {
    pub fn parse_and_format(&self, data: &[u8], endian: Endian) -> Option<String> {
        if self.size.is_some() && Some(data.len()) != self.size {
            return None;
        }

        Some(self.pretty_parser?(data, endian))
    }

    pub fn has_pretty_formatter(&self) -> bool {
        self.pretty_parser.is_some() && self.size.is_some()
    }

    pub fn array_size(&self, array_length: usize) -> Option<usize> {
        self.size.map(|s| s * array_length)
    }
}

macro_rules! class {
    // class!(0x80800009 byte)
    ($id:literal $name:ident) => {
        class_internal!($id $name @size(None) @parse(None) @block_tags(false))
    };
    // class!(0x80800009 byte @size(1))
    ($id:literal $name:ident @size($size:expr)) => {
        class_internal!($id $name @size(Some($size)) @parse(Some(parse_hex)) @block_tags(false))
    };
    // class!(0x80800009 byte @size(1) @block_tags)
    ($id:literal $name:ident @size($size:expr) @block_tags) => {
        class_internal!($id $name @size(Some($size)) @parse(Some(parse_hex)) @block_tags(true))
    };
    // class!(0x80800009 byte @size(1) @parse(parse_u8))
    ($id:literal $name:ident @size($size:expr) @parse($parsefn:expr)) => {
        class_internal!($id $name @size(Some($size)) @parse(Some($parsefn)) @block_tags(false))
    };
    // class!(0x80800009 byte @size(1) @parse(parse_u8) @block_tags)
    ($id:literal $name:ident @size($size:expr) @parse($parsefn:expr) @block_tags) => {
        class_internal!($id $name @size(Some($size)) @parse(Some($parsefn)) @block_tags(true))
    };
}

macro_rules! class_internal {
    ($id:literal $name:ident @size($size:expr) @parse($parsefn:expr) @block_tags($block_tags:expr)) => {
        TagClass {
            id: $id,
            name: Cow::Borrowed(stringify!($name)),
            size: $size,
            pretty_parser: $parsefn,
            block_tags: $block_tags,
        }
    };
}

pub const CLASSES_BASE: &[TagClass] = &[
    class!(0x80800000 s_bungie_script),
    class!(0x80800005 char @size(1) @block_tags),
    class!(0x80800007 u32 @size(4) @parse(parse_raw_hex::<u32>) @block_tags),
    class!(0x80800009 byte @size(1) @parse(parse_raw_hex::<u8>) @block_tags),
    class!(0x8080000A u16 @size(2) @parse(parse_raw_hex::<u16>) @block_tags),
    class!(0x80800014 taghash @size(4) @parse(parse_taghash)),
    class!(0x80800070 f32 @size(4) @parse(parse_raw::<f32>) @block_tags),
    class!(0x80800090 vec4 @size(16) @parse(parse_vec4) @block_tags),
];

pub const CLASSES_DESTINY_DEVALPHA: &[TagClass] = &[
    class!(0x808004A8 s_localized_strings),
    class!(0x808004A6 s_localized_strings_data),
];

pub const CLASSES_DESTINY_TTK: &[TagClass] = &[
    class!(0x8080035A s_localized_strings),
    class!(0x80800734 s_entity),
    class!(0x80800861 s_entity_resource),
    class!(0x808008BE s_localized_strings_data),
    class!(0x80801AD0 s_scope),
    class!(0x80801B4C s_technique),
];

pub const CLASSES_DESTINY_ROI: &[TagClass] = &[
    class!(0x8080035A s_localized_strings),
    class!(0x808008BE s_localized_strings_data),
    class!(0x80801A7A s_hdao_settings),
    class!(0x80801AB2 s_screen_area_fx_settings),
    class!(0x80801AD7 s_technique),
    class!(0x80801AF4 s_gear_dye),
    class!(0x80801B2B s_post_process_settings),
    class!(0x80801BC1 s_autoexposure_settings),
    class!(0x80801C47 s_scope),
    class!(0x80802732 sui_tab_list),
    class!(0x808033EB sui_simple_dialog),
];

pub const CLASSES_DESTINY_SK: &[TagClass] = &[
    class!(0x80804607 s_unk80804607 @size(12)),
    class!(0x80804616 s_unk80804616 @size(4)),
    class!(0x80804618 s_unk80804618 @size(4)),
    class!(0x80804622 s_unk80804622 @size(16)),
    class!(0x808046D4 s_unk808046d4 @size(24)),
    class!(0x808046F3 s_unk808046f3 @size(24)),
    class!(0x8080473B s_unk8080473b @size(16)),
    class!(0x80804743 s_unk80804743 @size(24)),
    class!(0x80804747 s_unk80804747 @size(8) @parse(parse_raw_unchecked::<(FnvHash, TagHash)>)),
    class!(0x8080474f s_unk8080474f @size(8)),
    class!(0x80804756 s_unk80804756 @size(128)),
    class!(0x8080475f s_unk8080475f @size(40)),
    class!(0x80804762 s_unk80804762 @size(8)),
    class!(0x80804763 s_unk80804763 @size(8)),
    class!(0x80804767 s_unk80804767 @size(8)),
    class!(0x80804768 s_unk80804768 @size(64)),
    class!(0x80804770 s_unk80804770 @size(32)),
    class!(0x80804772 s_unk80804772 @size(32)),
    class!(0x808047C6 s_unk808047c6 @size(8)),
    class!(0x808047CB s_unk808047cb @size(24)),
    class!(0x808047CD s_unk808047cd @size(8)),
    class!(0x80804858 s_unk80804858 @size(24)),
    class!(0x808048D7 s_unk808048d7 @size(32)),
    class!(0x80806B10 s_unk80806b10 @size(12)),
    class!(0x80807190 s_static_mesh_instance_group),
    class!(0x80807193 s_static_special_mesh),
    class!(0x80807194 s_static_mesh_data),
    class!(0x8080719A s_static_mesh_part),
    class!(0x8080719B s_static_mesh_group),
    class!(0x808071A3 s_static_instance_transform),
    class!(0x808071A7 s_static_mesh),
    class!(0x808071E8 s_technique),
    class!(0x80807211 s_material_texture_assignment),
    class!(0x808073F3 s_sampler_reference),
    class!(0x8080966D s_static_mesh_instances),
    class!(0x808071F3 s_scope),
    class!(0x80809802 s_wwise_event),
    class!(0x80809A88 s_localized_strings),
    class!(0x80809A8A s_localized_strings_data),
];

pub const CLASSES_DESTINY_BL: &[TagClass] = &[
    class!(0x808045EB s_music_score),
    class!(0x80804F2C s_dye_channel_hash),
    class!(0x808051F2 s_dye_channels),
    class!(0x80806695 cubemap_resource),
    class!(0x80806A0D s_static_map_parent),
    class!(0x80806C81 s_terrain),
    class!(0x80806C84 s_static_part),
    class!(0x80806C86 s_mesh_group),
    class!(0x80806CC9 s_map_data_resource),
    class!(0x80806D28 s_static_mesh_instance_map),
    class!(0x80806D2F s_static_mesh_decal),
    class!(0x80806D30 s_static_mesh_data),
    class!(0x80806D36 s_static_mesh_buffers @size(16)),
    class!(0x80806D37 s_static_mesh_part @size(12)),
    class!(0x80806D38 s_static_mesh_group @size(6)),
    class!(0x80806D40 s_static_mesh_instance_transform),
    class!(0x80806D44 s_static_mesh),
    class!(0x80806DAA s_technique),
    class!(0x80806DBA s_scope),
    class!(0x80806DCF s_texture_tag_64 @size(24)),
    class!(0x80806EC5 s_entity_model_mesh),
    class!(0x80806F07 s_entity_model),
    class!(0x80807211 s_texture_tag),
    class!(0x80808701 s_bubble_definition),
    class!(0x80808703 s_map_container_entry),
    class!(0x80808707 s_map_container),
    class!(0x80808709 s_map_data_table_entry),
    class!(0x8080891E s_bubble_parent),
    class!(0x80808BE0 s_animation_clip),
    class!(0x80808E8E s_activity),
    class!(0x808093AD s_static_map_data),
    class!(0x808093B1 s_occlusion_bounds),
    class!(0x808093B3 s_mesh_instance_occlusion_bounds),
    class!(0x808093BD s_static_mesh_hash),
    class!(0x80809738 s_wwise_event),
    class!(0x808097B8 s_dialog_table),
    class!(0x80809883 s_map_data_table),
    class!(0x80809885 s_map_data_entry),
    class!(0x808099EF s_localized_strings),
    class!(0x808099F1 s_localized_strings_data),
    class!(0x808099F5 s_string_part_definition),
    class!(0x808099F7 s_string_part),
    class!(0x80809AD8 s_entity),
    class!(0x80809B06 s_entity_resource),
    class!(0x8080BFE6 s_unk_music_8080bfe6),
    class!(0x8080BFE8 s_unk_music_8080bfe8),
    class!(0x80806920 s_gpu_particle_system),
    // huge array in the umbra tome tags
    class!(0x80806E89 s_unk_80806e89 @size(16) @block_tags),
];

// TODO(cohae): User-defined references
lazy_static::lazy_static! {
    static ref CLASS_MAP: ArcSwap<FxHashMap<u32, TagClass>> = ArcSwap::new(Default::default());
    static ref CLASS_MAP_FROM_FILE: ArcSwap<FxHashMap<u32, TagClass>> = ArcSwap::new(Default::default());
    static ref REFRESHED_THIS_FRAME: AtomicBool = AtomicBool::new(false);
}

pub fn get_class_by_id(id: u32) -> Option<TagClass> {
    CLASS_MAP
        .load()
        .get(&id)
        .cloned()
        .or_else(|| CLASS_MAP_FROM_FILE.load().get(&id).cloned())
}

pub fn load_schemafile() {
    let Ok(schemafile) = std::fs::read_to_string("schema.txt") else {
        return;
    };

    match parse_schemafile(&schemafile) {
        Ok(o) => {
            CLASS_MAP_FROM_FILE.store(Arc::new(o));
            REFRESHED_THIS_FRAME.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Err(e) => {
            log::error!("Failed to parse schema file: {:?}", e);
        }
    }
}

pub fn was_schemafile_refreshed() -> bool {
    REFRESHED_THIS_FRAME.load(std::sync::atomic::Ordering::Relaxed)
}

fn parse_schemafile(s: &str) -> anyhow::Result<FxHashMap<u32, TagClass>> {
    let mut schema: FxHashMap<u32, TagClass> = Default::default();

    // schema.txt lines can either be formatted as:
    // 8080XXXX <name>
    // 8080XXXX <name> <size>
    for l in s.lines() {
        let mut parts = l.split_whitespace();
        let id = u32::from_str_radix(parts.next().context("Missing class ID")?, 16)?;
        let name = parts.next().context("Missing name")?;
        let size = parts
            .next()
            .map(|s| s.parse().context("Failed to parse size"))
            .transpose()?;

        schema.insert(
            id,
            TagClass {
                id,
                name: Cow::Owned(name.to_string()),
                size,
                pretty_parser: Some(parse_hex),
                block_tags: false,
            },
        );
    }

    Ok(schema)
}

pub fn initialize_reference_names() {
    if package_manager_checked().is_err() {
        panic!("Called initialize_reference_names, but package manager is not initialized!")
    }

    let mut new_classes: FxHashMap<u32, TagClass> =
        CLASSES_BASE.iter().map(|c| (c.id, c.clone())).collect();

    let version_specific = match package_manager().version {
        GameVersion::Destiny(DestinyVersion::DestinyInternalAlpha) => CLASSES_DESTINY_DEVALPHA,
        GameVersion::Destiny(DestinyVersion::DestinyTheTakenKing) => CLASSES_DESTINY_TTK,
        GameVersion::Destiny(DestinyVersion::DestinyFirstLookAlpha)
        | GameVersion::Destiny(DestinyVersion::DestinyRiseOfIron) => CLASSES_DESTINY_ROI,
        GameVersion::Destiny(DestinyVersion::Destiny2Beta)
        | GameVersion::Destiny(DestinyVersion::Destiny2Forsaken)
        | GameVersion::Destiny(DestinyVersion::Destiny2Shadowkeep) => CLASSES_DESTINY_SK,
        GameVersion::Destiny(DestinyVersion::Destiny2BeyondLight)
        | GameVersion::Destiny(DestinyVersion::Destiny2WitchQueen)
        | GameVersion::Destiny(DestinyVersion::Destiny2Lightfall)
        | GameVersion::Destiny(DestinyVersion::Destiny2TheFinalShape) => CLASSES_DESTINY_BL,
        _ => unimplemented!(),
    };

    new_classes.extend(version_specific.iter().map(|c| (c.id, c.clone())));

    CLASS_MAP.store(Arc::new(new_classes));
}

const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";

// the length of data has to be guaranteed to be the TagClass::size for the type it's trying to parse
// parse_hex is used as default parser for all classes
fn parse_hex(data: &[u8], _: Endian) -> String {
    let mut result = String::with_capacity(data.len() * 2);

    for &b in data {
        result.push(HEX_CHARS[((b >> 4) & 0xf) as usize] as char);
        result.push(HEX_CHARS[(b & 0xf) as usize] as char);
        result.push(' ');
    }

    result
}

fn parse_raw_unchecked<T: Sized + Debug>(data: &[u8], endian: Endian) -> String {
    assert!(data.len() >= size_of::<T>());
    let mut bytes = data[0..size_of::<T>()].to_vec();
    if endian != Endian::NATIVE {
        bytes.reverse();
    }
    let v: T = unsafe { (bytes.as_ptr() as *const T).read() };
    format!("{v:X?}")
}

fn parse_raw<T: Sized + Pod + Display>(data: &[u8], endian: Endian) -> String {
    assert!(data.len() >= size_of::<T>());
    let mut bytes = data[0..size_of::<T>()].to_vec();
    if endian != Endian::NATIVE {
        bytes.reverse();
    }
    let v: T = bytemuck::try_pod_read_unaligned(&bytes).unwrap();
    v.to_string()
}

fn parse_raw_hex<T: Sized + Pod + UpperHex>(data: &[u8], endian: Endian) -> String {
    assert!(data.len() >= size_of::<T>());
    let mut bytes = data[0..size_of::<T>()].to_vec();
    if endian != Endian::NATIVE {
        bytes.reverse();
    }
    let v: T = bytemuck::try_pod_read_unaligned(&bytes).unwrap();
    format!("0x{v:X}")
}

fn parse_taghash(data: &[u8], endian: Endian) -> String {
    let taghash = TagHash(u32_from_endian(endian, data.try_into().unwrap()));
    format!("tag({taghash})")
}

fn parse_vec4(data: &[u8], endian: Endian) -> String {
    let from_bytes = match endian {
        Endian::Big => f32::from_be_bytes,
        Endian::Little => f32::from_le_bytes,
    };

    let vec4: [f32; 4] = [
        from_bytes(data[0..4].try_into().unwrap()),
        from_bytes(data[4..8].try_into().unwrap()),
        from_bytes(data[8..12].try_into().unwrap()),
        from_bytes(data[12..16].try_into().unwrap()),
    ];

    format!("vec4({}, {}, {}, {})", vec4[0], vec4[1], vec4[2], vec4[3])
}

#[derive(Pod, Zeroable, Clone, Copy)]
#[repr(transparent)]
pub struct FnvHash(u32);

impl Debug for FnvHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fnv({:08X})", self.0)
    }
}
