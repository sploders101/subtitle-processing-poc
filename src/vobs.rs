//! Written from the docs at this page:
//!
//! https://sam.zoy.org/writings/dvd/subtitles/

use image::{Rgb, Rgba, RgbaImage};

use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum SubsError {
    #[error("The VobSub idx data is invalid.")]
    InvalidIdx,
    #[error("Invalid VobSub frame header.")]
    InvalidFrameHeader,
    #[error("Invalid VobSub control data.")]
    InvalidControl,
    #[error("Invalid VobSub frame data.")]
    InvalidFrame,
}

pub struct IdxData {
    pub palette: [Rgb<u8>; 16],
}
pub fn parse_idx(data: &[u8]) -> Result<IdxData, SubsError> {
    for line in String::from_utf8_lossy(data).split("\n") {
        if line.trim_start().starts_with("#") {
            continue;
        }
        let (key, value) = line.split_once(": ").ok_or(SubsError::InvalidIdx)?;
        if key == "palette" {
            return Ok(IdxData {
                palette: parse_palette(value).ok_or(SubsError::InvalidIdx)?,
            });
        }
    }
    return Err(SubsError::InvalidIdx);
}

pub fn parse_palette(palette: &str) -> Option<[Rgb<u8>; 16]> {
    let segments = palette.split(",");
    let mut palette = [Rgb::<u8>([0, 0, 0]); 16];
    for (i, segment) in segments.enumerate() {
        hex::decode_to_slice(segment.trim(), &mut palette[i].0).ok()?;
    }
    return Some(palette);
}

pub fn parse_frame(idx: &IdxData, file_data: &[u8]) -> Result<RgbaImage, SubsError> {
    if file_data.len() < 4 {
        return Err(SubsError::InvalidFrameHeader);
    }
    let _file_size = u16::from_be_bytes([file_data[0], file_data[1]]);
    let control_offset = u16::from_be_bytes([file_data[2], file_data[3]]);

    let control =
        parse_control(&file_data, control_offset as usize).ok_or(SubsError::InvalidControl)?;
    return parse_data(&idx.palette, control, &file_data).ok_or(SubsError::InvalidFrame);
}

#[derive(Debug, Clone)]
pub struct Coordinates {
    pub x1: u16,
    pub x2: u16,
    pub y1: u16,
    pub y2: u16,
}

#[derive(Default, Debug, Clone)]
pub struct ControlData {
    pub force: bool,
    pub start_time: Option<u16>,
    pub stop_time: Option<u16>,
    pub color_palette: Option<[u8; 4]>,
    pub alpha_palette: Option<[u8; 4]>,
    pub coordinates: Option<Coordinates>,
    pub rle_offsets: Option<(u16, u16)>,
}

fn parse_control(data: &[u8], mut cursor: usize) -> Option<ControlData> {
    let mut control = ControlData::default();
    loop {
        if data.len() <= cursor + 4 {
            return None;
        }
        let this_sequence = cursor;
        let offset_time = u16::from_be_bytes([data[cursor + 0], data[cursor + 1]]);
        let next_control = u16::from_be_bytes([data[cursor + 2], data[cursor + 3]]);
        cursor += 4;
        loop {
            if data.len() <= cursor {
                return None;
            }
            let command = data[cursor];
            match command {
                0x00 => {
                    // Force displaying
                    control.force = true;
                    cursor += 1;
                }
                0x01 => {
                    // Start date
                    control.start_time = Some(offset_time);
                    cursor += 1;
                }
                0x02 => {
                    // Stop date
                    control.stop_time = Some(offset_time);
                    cursor += 1;
                }
                0x03 => {
                    // Palette
                    let mut colors = [0u8; 4];
                    let mut nibbles = NibbleStream::new(&data[cursor + 1..cursor + 3]);
                    for i in 0..4 {
                        colors[i] = nibbles.take_nibble()?;
                    }
                    control.color_palette = Some(colors);
                    cursor += 3;
                }
                0x04 => {
                    // Alpha channel
                    let mut alphas = [0u8; 4];
                    let mut nibbles = NibbleStream::new(&data[cursor + 1..cursor + 3]);
                    for i in 0..4 {
                        alphas[i] = nibbles.take_nibble()?;
                    }
                    control.alpha_palette = Some(alphas);
                    cursor += 3;
                }
                0x05 => {
                    // Coordinates
                    if data.len() <= cursor + 6 {
                        return None;
                    }
                    let coordinates = Coordinates {
                        x1: u16::from_be_bytes([data[cursor + 1], data[cursor + 2]]) >> 4 & 0xFFF,
                        x2: u16::from_be_bytes([data[cursor + 2], data[cursor + 3]]) & 0xFFF,
                        y1: u16::from_be_bytes([data[cursor + 4], data[cursor + 5]]) >> 4 & 0xFFF,
                        y2: u16::from_be_bytes([data[cursor + 5], data[cursor + 6]]) & 0xFFF,
                    };
                    control.coordinates = Some(coordinates);
                    cursor += 7;
                }
                0x06 => {
                    // RLE offsets
                    if data.len() <= cursor + 4 {
                        return None;
                    }
                    let evens = u16::from_be_bytes([data[cursor + 1], data[cursor + 2]]);
                    let odds = u16::from_be_bytes([data[cursor + 3], data[cursor + 4]]);
                    control.rle_offsets = Some((evens, odds));
                    cursor += 5;
                }
                0xFF => {
                    // End of command sequence
                    break;
                }
                _ => {}
            }
        }
        if next_control as usize == this_sequence {
            break;
        } else {
            cursor = next_control as usize;
        }
    }
    return Some(control);
}

#[derive(Debug, Clone, Copy)]
struct Rle {
    length: u32,
    color: u8,
}
fn read_rle(nibble_stream: &mut NibbleStream) -> Option<Rle> {
    let n = match nibble_stream.take_nibble()? {
        n1 @ 0x4..=0xf => n1 as u16,
        n1 @ 0x1..=0x3 => {
            let n2 = nibble_stream.take_nibble()?;
            let n = (n1 << 4) | n2;
            n as u16
        }
        0x0 => match nibble_stream.take_nibble()? {
            n2 @ 0x4..=0xf => {
                let n2 = n2 as u8;
                let n3 = nibble_stream.take_nibble()? as u8;
                ((n2 << 4) | n3) as u16
            }
            n2 @ 0x0..=0x3 => {
                let n2 = n2;
                let n3 = nibble_stream.take_nibble()?;
                let n4 = nibble_stream.take_nibble()?;
                u16::from_be_bytes([n2, (n3 << 4) | n4])
            }
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    return Some(Rle {
        length: (n >> 2) as u32,
        color: (n & 0x3) as u8,
    });
}

fn parse_data(
    palette: &[Rgb<u8>; 16],
    control: ControlData,
    data: &[u8],
) -> Option<image::ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let color_palette = control.color_palette?;
    let alpha_palette = control.alpha_palette?;
    let coordinates = control.coordinates?;
    let width = (coordinates.x2 - coordinates.x1 + 1) as u32;
    let height = (coordinates.y2 - coordinates.y1 + 1) as u32;
    let mut image = image::ImageBuffer::<Rgba<u8>, Vec<u8>>::new(width as _, height as _);

    let mut y = 0;

    let offsets = control.rle_offsets?;
    if data.len() <= offsets.0 as usize || data.len() <= offsets.1 as usize {
        return None;
    }
    let mut nibble_streams = [
        NibbleStream::new(&data[offsets.0 as usize..]),
        NibbleStream::new(&data[offsets.1 as usize..]),
    ];

    while y < height {
        let this_stream = &mut nibble_streams[(y % 2) as usize];
        // Read a whole line
        let mut x = 0;
        while x < width {
            let mut next_rle = read_rle(this_stream)?;
            if next_rle.length > width - x {
                return None;
            }
            if next_rle.length == 0 {
                this_stream.byte_align();
                next_rle.length = width - x;
            }
            for _ in 0..next_rle.length {
                // Color is a two-bit integer ranging from 0 through 3, and
                // the local palettes are 4 long, so no bounds check needed.
                let color_idx = color_palette[3 - next_rle.color as usize];
                let color_alpha = alpha_palette[3 - next_rle.color as usize];
                if color_idx >= 16 {
                    return None;
                }
                let color_opaque = palette[color_idx as usize].0;
                let color = Rgba([
                    color_opaque[0],
                    color_opaque[1],
                    color_opaque[2],
                    color_alpha,
                ]);
                image.put_pixel(x, y, color);
                x += 1;
            }
        }
        y += 1;
    }

    return Some(image);
}

/// Allows cursor-style reading of byte slices as u4 streams
pub struct NibbleStream<'a> {
    cursor: usize,
    data: &'a [u8],
}
impl<'a> NibbleStream<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        return Self { cursor: 0, data };
    }
    /// Ensures we are on a byte boundary, skipping a nibble
    /// if necessary.
    pub fn byte_align(&mut self) {
        if self.cursor % 2 != 0 {
            self.cursor += 1;
        }
    }
    /// Takes the next u4 from the stream
    pub fn take_nibble(&mut self) -> Option<u8> {
        let byte_cursor = self.cursor / 2;
        if self.data.len() <= byte_cursor {
            return None;
        }
        let start = self.data[byte_cursor];
        if self.cursor % 2 == 0 {
            self.cursor += 1;
            return Some(start >> 4 & 0xF);
        } else {
            self.cursor += 1;
            return Some(start & 0xF);
        }
    }
}
