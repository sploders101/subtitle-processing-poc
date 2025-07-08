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
use window_adapter::ImageWindow;

use crate::binary_reader::PacketReader;

mod constants;
mod pgs_types;
mod window_adapter;

fn render_into_image<'a>(
    image: &mut ImageWindow<'a>,
    palette: &HashMap<u8, image::LumaA<u8>>,
    data: &[u8],
) {
    let mut data = PacketReader::new(data);
    while let Some(leader) = data.read_u8() {
        match leader {
            0 => {
                let follower = data.read_u8().unwrap();
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
                        let l_cont = data.read_u8().unwrap();
                        let l = u16::from_be_bytes([follower_value, l_cont]);
                        for _ in 0..l {
                            image.push_pixel(image::LumaA([0, 0]));
                        }
                    }
                    0b10000000 => {
                        // L pixels in color C (L: 1-byte, C: 1-byte)
                        let l = follower_value;
                        let c = data.read_u8().unwrap();
                        let color = palette.get(&c).unwrap();
                        for _ in 0..l {
                            image.push_pixel(color.clone());
                        }
                    }
                    0b11000000 => {
                        // L pixels in color C (L: 2-byte, C: 1-byte)
                        let l_cont = data.read_u8().unwrap();
                        let l = u16::from_be_bytes([follower_value, l_cont]);
                        let c = data.read_u8().unwrap();
                        let color = palette.get(&c).unwrap();
                        for _ in 0..l {
                            image.push_pixel(color.clone());
                        }
                    }
                    _ => unreachable!(),
                }
            }
            c => {
                // One pixel in color
                let color = palette.get(&c).unwrap();
                image.push_pixel(color.clone());
            }
        }
    }
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
    pub fn process_mkv_frame(&mut self, frame: &Frame) -> Option<image::GrayAlphaImage> {
        // Parse display set
        let mut data = PacketReader::new(&frame.data);
        let display_set = read_display_set(&mut data);

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
            let palette = self.palette_table.get(&pcs.palette_id).unwrap();
            for object in pcs.composition_objects.iter() {
                let object_def = self.object_table.get(&object.object_id).unwrap();
                let window_def = self.window_table.get(&object.window_id).unwrap();
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
                render_into_image(&mut image_window, palette, &object_def.rle_data);
            }
            return Some(image);
        }

        // for ods in display_set.ods {
        //     let mut image = image::GrayAlphaImage::new(ods.width as _, ods.height as _);
        //     let mut image_window = ImageWindow::new(&mut image);
        //     render_into_image(
        //         &mut image_window,
        //         self.palette_table.get(&display_set.pcs.palette_id).unwrap(),
        //         &ods.rle_data,
        //     );
        //     print_gray_image(&image.convert());
        //     // return Some(image);
        // }

        return None;
    }
}

fn read_display_set<'a>(data: &mut PacketReader<'a>) -> PgsDisplaySet {
    let mut pcs: Option<PresentationComposition> = None;
    let mut wds: Vec<SingleWindowDefinition> = Vec::new();
    let mut pds: Vec<PaletteDefinition> = Vec::new();
    let mut ods: Vec<ObjectDefinition> = Vec::new();
    let mut current_ods: Option<ObjectDefinition> = None;
    loop {
        let segment_type = data.read_u8().unwrap();
        let segment_size = data.read_u16().unwrap();

        if data.get_remaining_bytes() < segment_size as usize {
            panic!("Segment length is greater than data length");
        }
        let data = data.take_bytes(segment_size as usize).unwrap();

        match segment_type {
            PGS_SEGMENT_TYPE_PDS => {
                pds.push(parse_pds(&data));
            }
            PGS_SEGMENT_TYPE_ODS => {
                let this_ods = parse_ods(&data);
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
                pcs = Some(parse_pcs(&data));
            }
            PGS_SEGMENT_TYPE_WDS => {
                wds.extend(parse_wds(&data));
            }
            PGS_SEGMENT_TYPE_END => {
                return PgsDisplaySet {
                    pcs: pcs.unwrap(),
                    wds,
                    pds,
                    ods,
                };
            }
            _ => panic!("Invalid segment type"),
        }
    }
}

fn parse_pds(data: &[u8]) -> PaletteDefinition {
    let mut data = PacketReader::new(data);
    let palette_id = data.read_u8().unwrap();
    let palette_version = data.read_u8().unwrap();
    let mut entries = Vec::new();
    while let Some(palette_entry_id) = data.read_u8() {
        entries.push(PaletteEntry {
            palette_entry_id,
            luminance: data.read_u8().unwrap(),
            color_diff_red: data.read_u8().unwrap(),
            color_diff_blue: data.read_u8().unwrap(),
            transparency: data.read_u8().unwrap(),
        });
    }
    return PaletteDefinition {
        palette_id,
        palette_version,
        entries,
    };
}
fn parse_ods(data: &[u8]) -> ObjectDefinition {
    let mut data = PacketReader::new(data);
    let object_id = data.read_u16().unwrap();
    let object_version = data.read_u8().unwrap();
    let last_in_sequence_flag = data.read_u8().unwrap();
    let object_data_length_buf = data.take_bytes(3).unwrap();
    let object_data_length = u32::from_be_bytes([
        0,
        object_data_length_buf[0],
        object_data_length_buf[1],
        object_data_length_buf[2],
    ])
    .saturating_sub(4); // Subtract size of width & height
    let width = data.read_u16().unwrap();
    let height = data.read_u16().unwrap();
    let rle_data = Vec::from(data.take_bytes(object_data_length as usize).unwrap());
    return ObjectDefinition {
        object_id,
        object_version,
        last_in_sequence: LastInSequence::from_bits(last_in_sequence_flag).unwrap(),
        width,
        height,
        rle_data,
    };
}
fn parse_pcs(data: &[u8]) -> PresentationComposition {
    let mut data = PacketReader::new(data);

    let width = data.read_u16().unwrap();
    let height = data.read_u16().unwrap();
    let frame_rate = data.read_u8().unwrap();
    let composition_number = data.read_u16().unwrap();
    let composition_state = match data.read_u8().unwrap() {
        0x00 => CompositionState::Normal,
        0x40 => CompositionState::AcquisitionPoint,
        0x80 => CompositionState::EpochStart,
        _ => panic!("Invalid composition state"),
    };
    let palette_update_flag = data.read_u8().unwrap() > 0;
    let palette_id = data.read_u8().unwrap();
    let composition_object_len = data.read_u8().unwrap();

    let mut composition_objects = Vec::new();
    for _ in 0..composition_object_len {
        let object_id = data.read_u16().unwrap();
        let window_id = data.read_u8().unwrap();
        let object_cropped_flag = data.read_u8().unwrap() & 0x80 > 0;
        let object_horizontal_pos = data.read_u16().unwrap();
        let object_vertical_pos = data.read_u16().unwrap();

        let object_cropping_horizontal_pos = if object_cropped_flag {
            data.read_u16().unwrap()
        } else {
            0
        };
        let object_cropping_vertical_pos = if object_cropped_flag {
            data.read_u16().unwrap()
        } else {
            0
        };
        let object_cropping_width = if object_cropped_flag {
            data.read_u16().unwrap()
        } else {
            0
        };
        let object_cropping_height = if object_cropped_flag {
            data.read_u16().unwrap()
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

    return PresentationComposition {
        width,
        height,
        frame_rate,
        composition_number,
        composition_state,
        palette_update_flag,
        palette_id,
        composition_objects,
    };
}
fn parse_wds(data: &[u8]) -> Vec<SingleWindowDefinition> {
    let mut data = PacketReader::new(data);
    let num_windows = data.read_u8().unwrap();
    let mut windows = Vec::new();
    for _ in 0..num_windows {
        windows.push(SingleWindowDefinition {
            window_id: data.read_u8().unwrap(),
            horizontal_pos: data.read_u16().unwrap(),
            vertical_pos: data.read_u16().unwrap(),
            width: data.read_u16().unwrap(),
            height: data.read_u16().unwrap(),
        });
    }
    return windows;
}
