//! This implements a PGS parser for the S_HDMV/PGS subtitle format.
//! It is intended to be used for parsing data from MKV files, though
//! it could be adapted to support other containers, or raw SUP files
//! as well.
//!
//! This code was implemented from the format described here:
//! https://blog.thescorpius.com/index.php/2017/07/15/presentation-graphic-stream-sup-files-bluray-subtitle-format/

use std::collections::HashMap;

use constants::{
    PGS_SEGMENT_TYPE_END, PGS_SEGMENT_TYPE_ODS, PGS_SEGMENT_TYPE_PCS, PGS_SEGMENT_TYPE_PDS,
    PGS_SEGMENT_TYPE_WDS,
};
use image::LumaA;
use matroska_demuxer::Frame;
use pgs_types::{
    CompositionObject, CompositionState, LastInSequence, ObjectDefinition, PaletteDefinition,
    PaletteEntry, PgsDisplaySet, PresentationComposition, SingleWindowDefinition,
};
use thiserror::Error;
use window_adapter::ImageWindow;

use crate::binary_reader::PacketReader;

mod constants;
mod pgs_types;
mod window_adapter;

#[derive(Error, Debug)]
pub enum PgsError {
    #[error("Palette {palette_id} missing in composition {composition_number}.")]
    MissingPalette {
        palette_id: u8,
        composition_number: u16,
    },
    #[error(
        "Color {color_id} missing from palette {palette_id}, referenced in composition {composition_number}."
    )]
    MissingColor {
        color_id: u8,
        palette_id: u8,
        composition_number: u16,
    },
    #[error("Object {object_id} missing in composition {composition_number}.")]
    MissingObject {
        object_id: u16,
        composition_number: u16,
    },
    #[error("Window {window_id} missing in composition {composition_number}.")]
    MissingWindow {
        window_id: u8,
        composition_number: u16,
    },
    #[error("Invalid RLE segment found.")]
    RleFormatError,
    #[error("Invalid PGS segment found.")]
    FormatError,
}

fn render_into_image<'a>(
    image: &mut ImageWindow<'a>,
    palette_id: u8,
    composition_number: u16,
    palette: &HashMap<u8, image::LumaA<u8>>,
    data: &[u8],
) -> Result<(), PgsError> {
    let mut data = PacketReader::new(data);
    while let Some(leader) = data.read_u8() {
        match leader {
            0 => {
                let follower = data.read_u8().ok_or(PgsError::RleFormatError)?;
                if follower == 0 {
                    // End of line
                    image.end_line();
                }
                let follower_code = follower & 0b11000000;
                let follower_value = follower & 0b00111111;
                match follower_code {
                    0b00000000 => {
                        // L pixels in color 0 (1-byte)
                        let l = follower_value;
                        for _ in 0..l {
                            image.push_pixel(image::LumaA([0, 0]));
                        }
                    }
                    0b01000000 => {
                        // L pixels in color 0 (2-byte)
                        let l_cont = data.read_u8().ok_or(PgsError::RleFormatError)?;
                        let l = u16::from_be_bytes([follower_value, l_cont]);
                        for _ in 0..l {
                            image.push_pixel(image::LumaA([0, 0]));
                        }
                    }
                    0b10000000 => {
                        // L pixels in color C (L: 1-byte, C: 1-byte)
                        let l = follower_value;
                        let c = data.read_u8().ok_or(PgsError::RleFormatError)?;
                        let color = palette.get(&c).ok_or(PgsError::MissingColor {
                            color_id: c,
                            palette_id,
                            composition_number,
                        })?;
                        for _ in 0..l {
                            image.push_pixel(color.clone());
                        }
                    }
                    0b11000000 => {
                        // L pixels in color C (L: 2-byte, C: 1-byte)
                        let l_cont = data.read_u8().ok_or(PgsError::RleFormatError)?;
                        let l = u16::from_be_bytes([follower_value, l_cont]);
                        let c = data.read_u8().ok_or(PgsError::RleFormatError)?;
                        let color = palette.get(&c).ok_or(PgsError::MissingColor {
                            color_id: c,
                            palette_id,
                            composition_number,
                        })?;
                        for _ in 0..l {
                            image.push_pixel(color.clone());
                        }
                    }
                    _ => unreachable!(),
                }
            }
            c => {
                // One pixel in color
                let color = palette.get(&c).ok_or(PgsError::MissingColor {
                    color_id: c,
                    palette_id,
                    composition_number,
                })?;
                image.push_pixel(color.clone());
            }
        }
    }
    return Ok(());
}

#[derive(Default)]
pub struct PgsParser {
    running_pcs: Option<PresentationComposition>,
    window_table: HashMap<u8, SingleWindowDefinition>,
    /// palette_id -> color_id -> color
    palette_table: HashMap<u8, HashMap<u8, LumaA<u8>>>,
    object_table: HashMap<u16, ObjectDefinition>,
}
impl PgsParser {
    pub fn new() -> Self {
        return PgsParser::default();
    }

    /// NOTE: This assumes frame times have already been scaled
    pub fn process_mkv_frame(
        &mut self,
        frame: &Frame,
    ) -> Result<Option<image::GrayAlphaImage>, PgsError> {
        // Parse display set
        let mut data = PacketReader::new(&frame.data);
        let display_set = read_display_set(&mut data)?;

        // Clear cache if requested
        if display_set.pcs.composition_state == CompositionState::EpochStart {
            // New epoch. Clear cache
            self.window_table.clear();
            self.palette_table.clear();
            self.object_table.clear();
        }

        // Update cache with new data
        for palette in display_set.pds {
            let stored_palette = match self.palette_table.get_mut(&palette.palette_id) {
                Some(palette) => palette,
                None => {
                    self.palette_table
                        .insert(palette.palette_id, HashMap::new());
                    // Unwrap here because we *just* added this entry
                    self.palette_table.get_mut(&palette.palette_id).unwrap()
                }
            };
            for entry in palette.entries {
                stored_palette.insert(
                    entry.palette_entry_id,
                    LumaA([entry.luminance, entry.transparency]),
                );
            }
        }
        for window in display_set.wds {
            self.window_table.insert(window.window_id, window);
        }
        for object in display_set.ods {
            self.object_table.insert(object.object_id, object);
        }

        // Update running PCS
        match display_set.pcs.composition_state {
            CompositionState::AcquisitionPoint => {
                if let Some(ref mut running_pcs) = self.running_pcs {
                    running_pcs.composition_number = display_set.pcs.composition_number;
                    running_pcs
                        .composition_objects
                        .extend(display_set.pcs.composition_objects);
                }
            }
            CompositionState::EpochStart | CompositionState::Normal => {
                self.running_pcs = Some(display_set.pcs);
            }
        }

        // Render PCS
        if let Some(ref pcs) = self.running_pcs {
            let mut image = image::GrayAlphaImage::new(pcs.width as _, pcs.height as _);
            let palette =
                self.palette_table
                    .get(&pcs.palette_id)
                    .ok_or(PgsError::MissingPalette {
                        palette_id: pcs.palette_id,
                        composition_number: pcs.composition_number,
                    })?;
            for object in pcs.composition_objects.iter() {
                let object_def =
                    self.object_table
                        .get(&object.object_id)
                        .ok_or(PgsError::MissingObject {
                            object_id: object.object_id,
                            composition_number: pcs.composition_number,
                        })?;
                let window_def =
                    self.window_table
                        .get(&object.window_id)
                        .ok_or(PgsError::MissingWindow {
                            window_id: object.window_id,
                            composition_number: pcs.composition_number,
                        })?;
                let mut image_window = if object.object_cropped_flag {
                    ImageWindow::with_window_cropped(
                        &mut image,
                        window_def.horizontal_pos as u32 + object.object_horizontal_pos as u32,
                        window_def.vertical_pos as u32 + object.object_vertical_pos as u32,
                        object.object_cropping_width as u32,
                        object.object_cropping_height as u32,
                        object.object_cropping_horizontal_pos as u32,
                        object.object_cropping_vertical_pos as u32,
                    )
                } else {
                    ImageWindow::with_window(
                        &mut image,
                        window_def.horizontal_pos as u32 + object.object_horizontal_pos as u32,
                        window_def.vertical_pos as u32 + object.object_vertical_pos as u32,
                        window_def.width as u32,
                        window_def.height as u32,
                    )
                };
                render_into_image(
                    &mut image_window,
                    pcs.palette_id,
                    pcs.composition_number,
                    palette,
                    &object_def.rle_data,
                );
            }
            return Ok(Some(image));
        }

        return Ok(None);
    }
}

fn read_display_set<'a>(data: &mut PacketReader<'a>) -> Result<PgsDisplaySet, PgsError> {
    let mut pcs: Option<PresentationComposition> = None;
    let mut wds: Vec<SingleWindowDefinition> = Vec::new();
    let mut pds: Vec<PaletteDefinition> = Vec::new();
    let mut ods: Vec<ObjectDefinition> = Vec::new();
    let mut current_ods: Option<ObjectDefinition> = None;
    loop {
        let segment_type = data.read_u8().ok_or(PgsError::FormatError)?;
        let segment_size = data.read_u16().ok_or(PgsError::FormatError)?;

        if data.get_remaining_bytes() < segment_size as usize {
            panic!("Segment length is greater than data length");
        }
        let data = data
            .take_bytes(segment_size as usize)
            .ok_or(PgsError::FormatError)?;

        match segment_type {
            PGS_SEGMENT_TYPE_PDS => {
                pds.push(parse_pds(&data)?);
            }
            PGS_SEGMENT_TYPE_ODS => {
                let this_ods = parse_ods(&data)?;
                if this_ods
                    .last_in_sequence
                    .contains(LastInSequence::FIRST_IN_SEQUENCE | LastInSequence::LAST_IN_SEQUENCE)
                {
                    if let Some(old_ods) = std::mem::take(&mut current_ods) {
                        ods.push(old_ods);
                    }
                    ods.push(this_ods);
                } else if this_ods
                    .last_in_sequence
                    .contains(LastInSequence::FIRST_IN_SEQUENCE)
                {
                    if let Some(old_ods) = std::mem::take(&mut current_ods) {
                        ods.push(old_ods);
                    }
                    current_ods = Some(this_ods);
                } else if this_ods
                    .last_in_sequence
                    .contains(LastInSequence::LAST_IN_SEQUENCE)
                {
                    if let Some(mut current_ods) = std::mem::take(&mut current_ods) {
                        current_ods.rle_data.extend(this_ods.rle_data);
                        ods.push(current_ods);
                    }
                } else {
                    if let Some(ref mut current_ods) = current_ods {
                        current_ods.rle_data.extend(this_ods.rle_data);
                    }
                }
            }
            PGS_SEGMENT_TYPE_PCS => {
                pcs = Some(parse_pcs(&data)?);
            }
            PGS_SEGMENT_TYPE_WDS => {
                wds.extend(parse_wds(&data)?);
            }
            PGS_SEGMENT_TYPE_END => {
                return Ok(PgsDisplaySet {
                    pcs: pcs.ok_or(PgsError::FormatError)?,
                    wds,
                    pds,
                    ods,
                });
            }
            _ => panic!("Invalid segment type"),
        }
    }
}

fn parse_pds(data: &[u8]) -> Result<PaletteDefinition, PgsError> {
    let mut data = PacketReader::new(data);
    let palette_id = data.read_u8().ok_or(PgsError::FormatError)?;
    let palette_version = data.read_u8().ok_or(PgsError::FormatError)?;
    let mut entries = Vec::new();
    while let Some(palette_entry_id) = data.read_u8() {
        entries.push(PaletteEntry {
            palette_entry_id,
            luminance: data.read_u8().ok_or(PgsError::FormatError)?,
            color_diff_red: data.read_u8().ok_or(PgsError::FormatError)?,
            color_diff_blue: data.read_u8().ok_or(PgsError::FormatError)?,
            transparency: data.read_u8().ok_or(PgsError::FormatError)?,
        });
    }
    return Ok(PaletteDefinition {
        palette_id,
        palette_version,
        entries,
    });
}
fn parse_ods(data: &[u8]) -> Result<ObjectDefinition, PgsError> {
    let mut data = PacketReader::new(data);
    let object_id = data.read_u16().ok_or(PgsError::FormatError)?;
    let object_version = data.read_u8().ok_or(PgsError::FormatError)?;
    let last_in_sequence_flag = data.read_u8().ok_or(PgsError::FormatError)?;
    let object_data_length_buf = data.take_bytes(3).ok_or(PgsError::FormatError)?;
    let object_data_length = u32::from_be_bytes([
        0,
        object_data_length_buf[0],
        object_data_length_buf[1],
        object_data_length_buf[2],
    ])
    .saturating_sub(4); // Subtract size of width & height
    let width = data.read_u16().ok_or(PgsError::FormatError)?;
    let height = data.read_u16().ok_or(PgsError::FormatError)?;
    let rle_data = Vec::from(
        data.take_bytes(object_data_length as usize)
            .ok_or(PgsError::FormatError)?,
    );
    return Ok(ObjectDefinition {
        object_id,
        object_version,
        last_in_sequence: LastInSequence::from_bits(last_in_sequence_flag)
            .ok_or(PgsError::FormatError)?,
        width,
        height,
        rle_data,
    });
}
fn parse_pcs(data: &[u8]) -> Result<PresentationComposition, PgsError> {
    let mut data = PacketReader::new(data);

    let width = data.read_u16().ok_or(PgsError::FormatError)?;
    let height = data.read_u16().ok_or(PgsError::FormatError)?;
    let frame_rate = data.read_u8().ok_or(PgsError::FormatError)?;
    let composition_number = data.read_u16().ok_or(PgsError::FormatError)?;
    let composition_state = match data.read_u8().ok_or(PgsError::FormatError)? {
        0x00 => CompositionState::Normal,
        0x40 => CompositionState::AcquisitionPoint,
        0x80 => CompositionState::EpochStart,
        _ => panic!("Invalid composition state"),
    };
    let palette_update_flag = data.read_u8().ok_or(PgsError::FormatError)? > 0;
    let palette_id = data.read_u8().ok_or(PgsError::FormatError)?;
    let composition_object_len = data.read_u8().ok_or(PgsError::FormatError)?;

    let mut composition_objects = Vec::new();
    for _ in 0..composition_object_len {
        let object_id = data.read_u16().ok_or(PgsError::FormatError)?;
        let window_id = data.read_u8().ok_or(PgsError::FormatError)?;
        let object_cropped_flag = data.read_u8().ok_or(PgsError::FormatError)? & 0x80 > 0;
        let object_horizontal_pos = data.read_u16().ok_or(PgsError::FormatError)?;
        let object_vertical_pos = data.read_u16().ok_or(PgsError::FormatError)?;

        let object_cropping_horizontal_pos = if object_cropped_flag {
            data.read_u16().ok_or(PgsError::FormatError)?
        } else {
            0
        };
        let object_cropping_vertical_pos = if object_cropped_flag {
            data.read_u16().ok_or(PgsError::FormatError)?
        } else {
            0
        };
        let object_cropping_width = if object_cropped_flag {
            data.read_u16().ok_or(PgsError::FormatError)?
        } else {
            0
        };
        let object_cropping_height = if object_cropped_flag {
            data.read_u16().ok_or(PgsError::FormatError)?
        } else {
            0
        };
        composition_objects.push(CompositionObject {
            object_id,
            window_id,
            object_cropped_flag,
            object_horizontal_pos,
            object_vertical_pos,
            object_cropping_horizontal_pos,
            object_cropping_vertical_pos,
            object_cropping_width,
            object_cropping_height,
        });
    }

    return Ok(PresentationComposition {
        width,
        height,
        frame_rate,
        composition_number,
        composition_state,
        palette_update_flag,
        palette_id,
        composition_objects,
    });
}
fn parse_wds(data: &[u8]) -> Result<Vec<SingleWindowDefinition>, PgsError> {
    let mut data = PacketReader::new(data);
    let num_windows = data.read_u8().ok_or(PgsError::FormatError)?;
    let mut windows = Vec::new();
    for _ in 0..num_windows {
        windows.push(SingleWindowDefinition {
            window_id: data.read_u8().ok_or(PgsError::FormatError)?,
            horizontal_pos: data.read_u16().ok_or(PgsError::FormatError)?,
            vertical_pos: data.read_u16().ok_or(PgsError::FormatError)?,
            width: data.read_u16().ok_or(PgsError::FormatError)?,
            height: data.read_u16().ok_or(PgsError::FormatError)?,
        });
    }
    return Ok(windows);
}
