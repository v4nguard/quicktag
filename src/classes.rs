use std::sync::Arc;

use arc_swap::{access::Access, ArcSwap};
use binrw::Endian;
use destiny_pkg::TagHash;
use eframe::epaint::mutex::RwLock;
use rustc_hash::FxHashMap;

use crate::{
    package_manager::{package_manager, package_manager_checked},
    util::u32_from_endian,
};

#[derive(Clone)]
pub struct TagClass {
    pub id: u32,
    pub name: &'static str,
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
            name: stringify!($name),
            size: $size,
            pretty_parser: $parsefn,
            block_tags: $block_tags,
        }
    };
}

pub const CLASSES_BASE: &[TagClass] = &[
    class!(0x80800000 s_bungie_script),
    class!(0x80800005 char @size(1) @block_tags),
    class!(0x80800007 u32 @size(4) @block_tags),
    class!(0x80800009 byte @size(1) @parse(parse_u8) @block_tags),
    class!(0x8080000A u16 @size(2) @parse(parse_u16) @block_tags),
    class!(0x80800014 taghash @size(4) @parse(parse_taghash)),
    class!(0x80800090 vec4 @size(16) @parse(parse_vec4) @block_tags),
];

pub const CLASSES_DEVALPHA: &[TagClass] = &[
    class!(0x808004A8 s_localized_strings),
    class!(0x808004A6 s_localized_strings_data),
];

pub const CLASSES_TTK: &[TagClass] = &[
    class!(0x8080035A s_localized_strings),
    class!(0x80800734 s_entity),
    class!(0x80800861 s_entity_resource),
    class!(0x808008BE s_localized_strings_data),
    class!(0x80801AD0 s_scope),
    class!(0x80801B4C s_technique),
];

pub const CLASSES_ROI: &[TagClass] = &[
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

pub const CLASSES_SK: &[TagClass] = &[];

pub const CLASSES_BL: &[TagClass] = &[
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
    // huge array in the umbra tome tags
    class!(0x80806E89 s_unk_80806e89 @size(16) @block_tags),
];

// TODO(cohae): User-defined references
lazy_static::lazy_static! {
    pub static ref CLASS_MAP: ArcSwap<FxHashMap<u32, TagClass>> = ArcSwap::new(Default::default());
}

pub fn initialize_reference_names() {
    if package_manager_checked().is_err() {
        panic!("Called initialize_reference_names, but package manager is not initialized!")
    }

    let mut new_classes: FxHashMap<u32, TagClass> =
        CLASSES_BASE.iter().map(|c| (c.id, c.clone())).collect();

    let version_specific = match package_manager().version {
        destiny_pkg::GameVersion::DestinyInternalAlpha => CLASSES_DEVALPHA,
        destiny_pkg::GameVersion::DestinyTheTakenKing => CLASSES_TTK,
        destiny_pkg::GameVersion::DestinyRiseOfIron => CLASSES_ROI,
        destiny_pkg::GameVersion::Destiny2Beta
        | destiny_pkg::GameVersion::Destiny2Forsaken
        | destiny_pkg::GameVersion::Destiny2Shadowkeep => CLASSES_SK,
        destiny_pkg::GameVersion::Destiny2BeyondLight
        | destiny_pkg::GameVersion::Destiny2WitchQueen
        | destiny_pkg::GameVersion::Destiny2Lightfall
        | destiny_pkg::GameVersion::Destiny2TheFinalShape => CLASSES_BL,
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

fn parse_u8(data: &[u8], _endian: Endian) -> String {
    format!("0x{:02X}", data[0])
}

fn parse_u16(data: &[u8], endian: Endian) -> String {
    let from_bytes = match endian {
        Endian::Big => u16::from_be_bytes,
        Endian::Little => u16::from_le_bytes,
    };

    let value = from_bytes([data[0], data[1]]);
    format!("0x{value:04X}")
}

fn parse_taghash(data: &[u8], endian: Endian) -> String {
    let taghash = TagHash(u32_from_endian(endian, data.try_into().unwrap()));
    taghash.to_string()
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
