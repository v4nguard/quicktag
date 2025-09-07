// TODO(cohae): This is all copied from alkahest, and needs to be moved into alkahest-data when it becomes available

use std::fmt::{Debug, Formatter, Write};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::ops::Deref;
use std::slice::Iter;

use binrw::{BinRead, BinReaderExt, BinResult, Endian, VecArgs};
use log::{error, warn};
use quicktag_core::util::FNV1_BASE;
use rustc_hash::{FxHashMap, FxHashSet};
use tiger_pkg::{DestinyVersion, GameVersion, MarathonVersion, TagHash, package_manager};

pub type TablePointer32<T> = _TablePointer<i32, u32, T>;
pub type TablePointer64<T> = _TablePointer<i64, u64, T>;
pub type TablePointer<T> = TablePointer64<T>;

pub type RelPointer32<T = ()> = _RelPointer<i32, T>;
pub type RelPointer64<T = ()> = _RelPointer<i64, T>;
pub type RelPointer<T = ()> = RelPointer64<T>;

#[derive(Clone)]
pub struct _TablePointer<O: Into<i64>, C: Into<u64>, T: BinRead> {
    offset_base: u64,
    offset: O,
    count: C,

    data: Vec<T>,
}

impl<'a, O: Into<i64>, C: Into<u64>, T: BinRead> BinRead for _TablePointer<O, C, T>
where
    C: BinRead + Copy,
    O: BinRead + Copy,
    C::Args<'a>: Default + Clone,
    O::Args<'a>: Default + Clone,
    T::Args<'a>: Default + Clone,
{
    type Args<'b> = ();

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        endian: Endian,
        _args: Self::Args<'_>,
    ) -> BinResult<Self> {
        let count: C = reader.read_type(endian)?;
        let offset_base = reader.stream_position()?;
        let offset: O = reader.read_type(endian)?;

        let offset_save = reader.stream_position()?;

        let seek64: i64 = offset.into();
        reader.seek(SeekFrom::Start(offset_base))?;
        reader.seek(SeekFrom::Current(seek64 + 16))?;

        let count64: u64 = count.into();
        let mut data = Vec::with_capacity(count64 as _);
        for _ in 0..count64 {
            data.push(reader.read_type(endian)?);
        }

        reader.seek(SeekFrom::Start(offset_save))?;

        Ok(_TablePointer {
            offset_base,
            offset,
            count,
            data,
        })
    }
}

impl<O: Into<i64> + Copy, C: Into<u64> + Copy, T: BinRead> _TablePointer<O, C, T> {
    pub fn iter(&self) -> Iter<'_, T> {
        self.data.iter()
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn data(&self) -> &[T] {
        &self.data
    }
}

impl<O: Into<i64> + Copy, C: Into<u64> + Copy, T: BinRead> Deref for _TablePointer<O, C, T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<'a, O: Into<i64> + Copy, C: Into<u64> + Copy, T: BinRead> IntoIterator
    for &'a _TablePointer<O, C, T>
{
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}

impl<O: Into<i64> + Copy, C: Into<u64> + Copy, T: BinRead + Debug> Debug
    for _TablePointer<O, C, T>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "TablePointer(address=0x{:x}, count={}",
            self.offset_base as i64 + self.offset.into(),
            self.count.into(),
        ))?;

        f.write_str(", data=")?;
        self.data.fmt(f)?;

        f.write_char(')')
    }
}

#[derive(Clone, Copy)]
pub struct _RelPointer<O: Into<i64>, T: BinRead> {
    offset_base: u64,
    offset: O,

    data: T,
}

impl<'a, O: Into<i64>, T: BinRead> BinRead for _RelPointer<O, T>
where
    O: BinRead + Copy,
    O::Args<'a>: Default + Clone,
    T::Args<'a>: Default + Clone,
{
    type Args<'b> = ();

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        endian: Endian,
        _args: Self::Args<'_>,
    ) -> BinResult<Self> {
        let offset_base = reader.stream_position()?;
        let offset: O = reader.read_type(endian)?;

        let offset_save = reader.stream_position()?;

        let seek64: i64 = offset.into();
        reader.seek(SeekFrom::Start(offset_base))?;
        reader.seek(SeekFrom::Current(seek64))?;

        let data = reader.read_type(endian)?;

        reader.seek(SeekFrom::Start(offset_save))?;

        Ok(_RelPointer {
            offset_base,
            offset,
            data,
        })
    }
}

impl<O: Into<i64> + Copy, T: BinRead> Deref for _RelPointer<O, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<O: Into<i64> + Copy, T: BinRead + Debug> Debug for _RelPointer<O, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "RelPointer(address=0x{:x}",
            self.offset_base as i64 + self.offset.into(),
        ))?;

        f.write_str(", data=")?;
        self.data.fmt(f)?;

        f.write_char(')')
    }
}

impl<O: Into<i64> + Copy, T: BinRead + Debug> From<_RelPointer<O, T>> for SeekFrom {
    fn from(val: _RelPointer<O, T>) -> Self {
        SeekFrom::Start((val.offset_base as i64 + val.offset.into()) as u64)
    }
}

impl<O: Into<i64> + Copy, T: BinRead + Debug> _RelPointer<O, T> {
    fn offset_absolute(&self) -> u64 {
        (self.offset_base as i64 + self.offset.into()) as u64
    }
}

#[derive(BinRead, Debug)]
pub struct StringContainer {
    pub file_size: u64,
    pub string_hashes: TablePointer<u32>,
    pub language_english: TagHash,
    pub language_japanese: TagHash,
    pub language_german: TagHash,
    pub language_french: TagHash,
    pub language_spanish: TagHash,
    pub language_spanish_latam: TagHash,
    pub language_italian: TagHash,
    pub language_korean: TagHash,
    pub language_chinese_traditional: TagHash,
    pub language_chinese_simplified: TagHash,
    pub language_portuguese: TagHash,
    pub language_polish: TagHash,
    pub language_russian: TagHash,
}

impl StringContainer {
    pub fn all_languages(&self) -> Vec<(&'static str, TagHash)> {
        vec![
            ("en", self.language_english),
            ("jp", self.language_japanese),
            ("de", self.language_german),
            ("fr", self.language_french),
            ("es", self.language_spanish),
            ("es_latam", self.language_spanish_latam),
            ("it", self.language_italian),
            ("ko", self.language_korean),
            ("zh_t", self.language_chinese_traditional),
            ("zh_s", self.language_chinese_simplified),
            ("pt", self.language_portuguese),
            ("pl", self.language_polish),
            ("ru", self.language_russian),
        ]
    }
}

#[derive(BinRead, Debug)]
#[br(import(old_format: bool))]
pub struct StringData {
    pub file_size: u64,
    pub string_parts: TablePointer<StringPart>,
    #[br(if(old_format))]
    pub _unk1: (u64, u64),
    pub _unk2: TablePointer<()>,
    pub string_data: TablePointer<u8>,
    pub string_combinations: TablePointer<StringCombination>,
}

#[derive(BinRead, Debug)]
pub struct StringCombination {
    pub data: RelPointer,
    pub part_count: i64,
}

#[derive(BinRead, Debug)]
pub struct StringPart {
    pub _unk0: u64,
    pub data: RelPointer,
    pub variable_hash: u32,

    /// String data length.
    /// This is always equal to or larger than the string length due to variable character lengths
    pub byte_length: u16,
    pub string_length: u16,
    pub cipher_shift: u16,

    pub _unk2: u16,
    pub _unk3: u32,
}

#[derive(BinRead, Debug)]
pub struct StringContainerD1 {
    pub file_size: u32,
    pub string_hashes: TablePointer32<u32>,
    pub language_english: TagHash,
}

#[derive(BinRead, Debug)]
pub struct StringDataD1 {
    #[br(assert(file_size < 0xf00000))]
    pub file_size: u32,
    pub string_parts: TablePointer32<StringPartD1>,
    pub _unk1: (u32, u32),
    pub _unk2: TablePointer32<()>,
    pub string_data: TablePointer32<u8>,
    pub string_combinations: TablePointer32<StringCombinationD1>,
}

#[derive(BinRead, Debug)]
pub struct StringPartD1 {
    pub _unk0: u32,
    pub data: RelPointer32,
    pub _unk1: u32,

    pub byte_length: u16,
    pub string_length: u16,
    pub cipher_shift: u16,
    pub _unk2: u16,
}

#[derive(BinRead, Debug)]
pub struct StringCombinationD1 {
    pub data: RelPointer32,
    pub part_count: i32,
}

#[derive(BinRead, Debug)]
pub struct StringDataD1Alpha {
    pub file_size: u32,
    pub _unk4: (u32, u32),
    /// Plain UTF16 string data
    pub string_data: TablePointer32<u16>,
    pub string_combinations: TablePointer32<StringCombinationD1Alpha>,
}

#[derive(BinRead, Debug)]
pub struct StringCombinationD1Alpha {
    pub string_parts: TablePointer32<StringPartD1Alpha>,
}

#[derive(BinRead, Debug)]
pub struct StringPartD1Alpha {
    pub _unk0: u32,
    pub _unk1: u32,
    pub _unk2: u32,

    pub data_start: RelPointer32,
    pub data_end: RelPointer32,
}

#[derive(BinRead, Debug)]
pub struct StringContainerD1FirstLook {
    pub file_size: u64,
    pub string_hashes: TablePointer<u32>,
    pub language_english: TagHash,
}

#[derive(BinRead, Debug)]
pub struct StringDataD1FirstLook {
    pub file_size: u64,
    pub _unk2: TablePointer<()>,
    pub string_data: TablePointer<u16>,
    pub string_combinations: TablePointer<StringCombinationD1FirstLook>,
}

#[derive(BinRead, Debug)]
pub struct StringCombinationD1FirstLook {
    pub part_count: i64,
    pub data: RelPointer,
}

#[derive(BinRead, Debug)]
pub struct StringPartD1FirstLook {
    pub _unk0: u64,
    pub variable_hash: u32,
    pub _unk2: u32,
    pub data: RelPointer,
    pub data_end: RelPointer,
}

/// Expects raw un-shifted data as input
pub fn decode_text(data: &[u8], cipher: u16) -> String {
    // cohae: Modern versions of D2 no longer use the cipher system, we can take a shortcut
    if cipher == 0 {
        return String::from_utf8_lossy(data).to_string();
    }

    let mut data_clone = data.to_vec();

    let mut off = 0;
    // TODO(cohae): Shifting doesn't work entirely yet, there's still some weird characters beyond starting byte 0xe0
    while off < data.len() {
        match data[off] {
            0..=0xbf => {
                data_clone[off] += cipher as u8;
                off += 1
            }
            0xc0..=0xdf => {
                data_clone[off + 1] += cipher as u8;
                off += 2
            }
            0xe0..=0xef => {
                data_clone[off + 2] += cipher as u8;
                off += 3
            }
            0xf0..=0xff => {
                data_clone[off + 3] += cipher as u8;
                off += 4
            }
        }
    }

    String::from_utf8_lossy(&data_clone).to_string()
}

pub fn create_stringmap() -> anyhow::Result<StringCache> {
    // TODO: Change this match to use ordered version checking after destiny-pkg 0.11
    match package_manager().version {
        // cohae: Rise of Iron uses the same string format as D2
        GameVersion::Destiny(DestinyVersion::DestinyRiseOfIron)
        | GameVersion::Destiny(DestinyVersion::Destiny2Beta)
        | GameVersion::Destiny(DestinyVersion::Destiny2Forsaken)
        | GameVersion::Destiny(DestinyVersion::Destiny2Shadowkeep)
        | GameVersion::Destiny(DestinyVersion::Destiny2BeyondLight)
        | GameVersion::Destiny(DestinyVersion::Destiny2WitchQueen)
        | GameVersion::Destiny(DestinyVersion::Destiny2Lightfall)
        | GameVersion::Destiny(DestinyVersion::Destiny2TheFinalShape)
        | GameVersion::Destiny(DestinyVersion::Destiny2TheEdgeOfFate)
        | GameVersion::Marathon(MarathonVersion::MarathonAlpha) => create_stringmap_d2(),
        GameVersion::Destiny(DestinyVersion::DestinyFirstLookAlpha) => {
            create_stringmap_d1_firstlook()
        }
        GameVersion::Destiny(DestinyVersion::DestinyTheTakenKing) => create_stringmap_d1(),
        GameVersion::Destiny(DestinyVersion::DestinyInternalAlpha) => {
            create_stringmap_d1_devalpha()
        }
    }
}

pub fn create_stringmap_d2() -> anyhow::Result<StringCache> {
    let reference_type = match package_manager().version {
        GameVersion::Destiny(v) => match v {
            DestinyVersion::DestinyInternalAlpha
            | DestinyVersion::DestinyFirstLookAlpha
            | DestinyVersion::DestinyTheTakenKing
            | DestinyVersion::DestinyRiseOfIron => 0x8080035A,
            DestinyVersion::Destiny2Beta
            | DestinyVersion::Destiny2Forsaken
            | DestinyVersion::Destiny2Shadowkeep => 0x80809A88,
            DestinyVersion::Destiny2BeyondLight
            | DestinyVersion::Destiny2WitchQueen
            | DestinyVersion::Destiny2Lightfall
            | DestinyVersion::Destiny2TheFinalShape
            | DestinyVersion::Destiny2TheEdgeOfFate => 0x808099EF,
        },
        GameVersion::Marathon(MarathonVersion::MarathonAlpha) => {
            error!("Marathon Alpha is not supported");
            return Ok(StringCache::default());
        }
    };

    let old_format = matches!(package_manager().version, GameVersion::Destiny(v) if v <= DestinyVersion::Destiny2BeyondLight);

    let mut tmp_map: FxHashMap<u32, FxHashSet<String>> = Default::default();
    for (t, _) in package_manager()
        .get_all_by_reference(reference_type)
        .into_iter()
    {
        let Ok(textset_header) = package_manager().read_tag_binrw::<StringContainer>(t) else {
            continue;
        };

        let Ok(data) = package_manager().read_tag(textset_header.language_english) else {
            continue;
        };
        let mut cur = Cursor::new(&data);
        let text_data: StringData = cur.read_le_args((old_format,))?;

        for (combination, hash) in text_data
            .string_combinations
            .iter()
            .zip(textset_header.string_hashes.iter())
        {
            let mut final_string = String::new();

            for ip in 0..combination.part_count {
                cur.seek(combination.data.into())?;
                cur.seek(SeekFrom::Current(ip * 0x20))?;
                let part: StringPart = cur.read_le()?;
                if part.variable_hash != 0x811c9dc5 {
                    final_string += &format!("<{:08X}>", part.variable_hash);
                } else {
                    cur.seek(part.data.into())?;
                    let mut data = vec![0u8; part.byte_length as usize];
                    cur.read_exact(&mut data)?;
                    final_string += &decode_text(&data, part.cipher_shift);
                }
            }

            if *hash == FNV1_BASE {
                if !final_string.is_empty() {
                    warn!("String '{final_string}' has an empty hash (0x{FNV1_BASE:X})");
                }
                continue;
            }

            tmp_map.entry(*hash).or_default().insert(final_string);
        }
    }

    Ok(tmp_map
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect())
}

pub fn create_stringmap_d1() -> anyhow::Result<StringCache> {
    let mut tmp_map: FxHashMap<u32, FxHashSet<String>> = Default::default();
    for (t, _) in package_manager()
        .get_all_by_reference(0x8080035A)
        .into_iter()
    {
        let Ok(textset_header) = package_manager().read_tag_binrw::<StringContainerD1>(t) else {
            continue;
        };

        let Ok(data) = package_manager().read_tag(textset_header.language_english) else {
            continue;
        };
        let mut cur = Cursor::new(&data);
        let text_data = match cur.read_be::<StringDataD1>() {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to read string data: {:?}", e);
                continue;
            }
        };

        for (combination, hash) in text_data
            .string_combinations
            .iter()
            .zip(textset_header.string_hashes.iter())
        {
            if *hash == 0x811c9dc5 {
                continue;
            }

            let mut final_string = String::new();

            for ip in 0..combination.part_count {
                cur.seek(combination.data.into())?;
                cur.seek(SeekFrom::Current((ip as i64) * 20))?;
                let part: StringPartD1 = cur.read_be()?;
                cur.seek(part.data.into())?;
                let mut data = vec![0u8; part.byte_length as usize];
                cur.read_exact(&mut data)?;
                final_string += &decode_text(&data, part.cipher_shift);
            }

            if *hash == FNV1_BASE {
                if !final_string.is_empty() {
                    warn!("String '{final_string}' has an empty hash (0x{FNV1_BASE:X})");
                }
                continue;
            }

            tmp_map.entry(*hash).or_default().insert(final_string);
        }
    }

    Ok(tmp_map
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect())
}

pub fn create_stringmap_d1_devalpha() -> anyhow::Result<StringCache> {
    let mut tmp_map: FxHashMap<u32, FxHashSet<String>> = Default::default();
    for (t, _) in package_manager()
        .get_all_by_reference(0x808004A8)
        .into_iter()
    {
        let textset_header = match package_manager().read_tag_binrw::<StringContainerD1>(t) {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to read string container: {:?}", e);
                continue;
            }
        };

        let Ok(data) = package_manager().read_tag(textset_header.language_english) else {
            continue;
        };
        let mut cur = Cursor::new(&data);
        let text_data = match cur.read_be::<StringDataD1Alpha>() {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to read string data: {:?}", e);
                continue;
            }
        };

        for (combination, hash) in text_data
            .string_combinations
            .iter()
            .zip(textset_header.string_hashes.iter())
        {
            if *hash == 0x811c9dc5 {
                continue;
            }

            let mut final_string = String::new();

            for part in combination.string_parts.iter() {
                cur.seek(part.data_start.into())?;
                let data_length =
                    (part.data_end.offset_absolute() - part.data_start.offset_absolute()) as usize;
                let data: Vec<u16> = cur.read_be_args(VecArgs {
                    count: data_length / 2,
                    inner: (),
                })?;

                final_string += &String::from_utf16_lossy(&data);
            }

            if *hash == FNV1_BASE {
                if !final_string.is_empty() {
                    warn!("String '{final_string}' has an empty hash (0x{FNV1_BASE:X})");
                }
                continue;
            }

            tmp_map.entry(*hash).or_default().insert(final_string);
        }
    }

    Ok(tmp_map
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect())
}

pub fn create_stringmap_d1_firstlook() -> anyhow::Result<StringCache> {
    let mut tmp_map: FxHashMap<u32, FxHashSet<String>> = Default::default();
    for (t, _) in package_manager()
        .get_all_by_reference(0x8080035A)
        .into_iter()
    {
        let Ok(textset_header) = package_manager().read_tag_binrw::<StringContainerD1FirstLook>(t)
        else {
            continue;
        };

        let Ok(data) = package_manager().read_tag(textset_header.language_english) else {
            continue;
        };
        let mut cur = Cursor::new(&data);
        let text_data = match cur.read_le::<StringDataD1FirstLook>() {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to read string data: {:?}", e);
                continue;
            }
        };

        for (combination, hash) in text_data
            .string_combinations
            .iter()
            .zip(textset_header.string_hashes.iter())
        {
            if *hash == 0x811c9dc5 {
                continue;
            }

            let mut final_string = String::new();

            for ip in 0..combination.part_count {
                cur.seek(combination.data.into())?;
                cur.seek(SeekFrom::Current(0x10))?;
                cur.seek(SeekFrom::Current(ip * 0x20))?;
                let part: StringPartD1FirstLook = cur.read_le()?;
                cur.seek(part.data.into())?;

                let len = part.data_end.offset_absolute() - part.data.offset_absolute();

                let mut data = vec![0u8; len as usize];
                cur.read_exact(&mut data)?;

                // Alignment always seems to be off here
                let data_u16: Vec<u16> = bytemuck::pod_collect_to_vec(&data);

                final_string += &String::from_utf16(&data_u16)?;
            }

            if *hash == FNV1_BASE {
                if !final_string.is_empty() {
                    warn!("String '{final_string}' has an empty hash (0x{FNV1_BASE:X})");
                }
                continue;
            }

            tmp_map.entry(*hash).or_default().insert(final_string);
        }
    }

    Ok(tmp_map
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect())
}

pub type StringCache = FxHashMap<u32, Vec<String>>;
pub type StringCacheVec = Vec<(u32, Vec<String>)>;
pub type RawStringHashCache = FxHashMap<u32, Vec<(String, bool)>>;
