//! This is a proof-of-concept for extracting vobsub subtitles from an MKV file.
//! It makes use of some private functions from the vobsub crate, and requires a
//! modified copy to export them.
//!
//! This is primarily created as a testing ground for integrating subtitle extraction
//! into mediacorral. The current version really only works for vobsub, and converts
//! the vobsub images into sixel images, printing them to the terminal.

use bdsup::PgsParser;
use image::{GrayAlphaImage, buffer::ConvertBuffer};
use matroska_demuxer::*;
use sixel::print_gray_image;
use std::fs::File;

mod bdsup;
mod binary_reader;
mod sixel;
mod tess;
mod vobs;

fn main() {
    let file = File::open("test_bd.mkv").unwrap();
    let mut mkv = MatroskaFile::open(file).unwrap();
    let video_track = mkv
        .tracks()
        .iter()
        .find(|t| t.track_type() == TrackType::Subtitle)
        // .inspect(|t| {
        //     dbg!(t.codec_id());
        //     dbg!(t.codec_name());
        // })
        .unwrap()
        .clone();
    let timestamp_scale = mkv.info().timestamp_scale().get();
    let track_num = video_track.track_number().get();
    let mut sub_reader = PgsParser::new();

    let mut frame = Frame::default();
    while mkv.next_frame(&mut frame).unwrap() {
        if frame.track != track_num {
            continue;
        }
        frame.timestamp = frame.timestamp * timestamp_scale;
        frame.duration = frame.duration.map(|duration| duration * timestamp_scale);
        if let Some(image) = sub_reader.process_mkv_frame(&frame) {
            print_gray_image(&crop_image(&image).convert());
        }
    }
}

fn crop_image(image: &GrayAlphaImage) -> GrayAlphaImage {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..image.height() {
        for x in 0..image.width() {
            let pixel = image.get_pixel(x, y);
            if pixel.0[1] > 0 {
                match bounds {
                    Some((ref mut x1, _y1, ref mut x2, ref mut y2)) => {
                        if *x1 > x {
                            *x1 = x;
                        }
                        if *x2 < x {
                            *x2 = x;
                        }
                        // y1 not needed due to scanning semantics
                        if *y2 < y {
                            *y2 = y;
                        }
                    }
                    None => {
                        bounds = Some((x, y, x, y));
                    }
                }
            }
        }
    }
    match bounds {
        None => {
            return GrayAlphaImage::new(0, 0);
        }
        Some((x1, y1, x2, y2)) => {
            let mut new_image = GrayAlphaImage::new(x2 + 1 - x1, y2 + 1 - y1);
            for (new_y, y) in (y1..=y2).enumerate() {
                for (new_x, x) in (x1..=x2).enumerate() {
                    new_image.put_pixel(new_x as _, new_y as _, image.get_pixel(x, y).clone());
                }
            }
            return new_image;
        }
    }
}
