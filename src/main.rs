//! This is a proof-of-concept for extracting vobsub subtitles from an MKV file.
//! It makes use of some private functions from the vobsub crate, and requires a
//! modified copy to export them.
//!
//! This is primarily created as a testing ground for integrating subtitle extraction
//! into mediacorral. The current version really only works for vobsub, and converts
//! the vobsub images into sixel images, printing them to the terminal.

use image::{GrayImage, Pixel, RgbaImage};
use matroska_demuxer::*;
use sixel::print_gray_image;
use std::fs::File;

mod sixel;
mod tess;
mod vobs;

fn main() {
    let file = File::open("test.mkv").unwrap();
    let mut mkv = MatroskaFile::open(file).unwrap();
    dbg!(mkv.tags());
    let video_track = mkv
        .tracks()
        .iter()
        .find(|t| t.track_type() == TrackType::Subtitle)
        .inspect(|t| {
            dbg!(t.codec_id());
            dbg!(t.codec_name());
        })
        .unwrap()
        .clone();
    let track_num = video_track.track_number().get();
    let vobs_idx = video_track
        .codec_private()
        .map(|idx| vobs::parse_idx(idx).unwrap());

    let mut frame = Frame::default();
    while mkv.next_frame(&mut frame).unwrap() {
        if frame.track == track_num {
            match video_track.codec_id() {
                "S_SUBRIP" => {
                    println!(
                        "video frame found: {}",
                        String::from_utf8(frame.data.clone()).unwrap()
                    );
                }
                "S_VOBSUB" => {
                    let idx = vobs_idx.as_ref().unwrap();
                    let result = vobs::parse_frame(idx, &frame.data).unwrap();
                    let result = crop_image(&result);
                    // print_rgba_image(&result);
                    let mut gray_image: GrayImage = GrayImage::new(result.width(), result.height());

                    for (src_pixel, dest_pixel) in result.pixels().zip(gray_image.pixels_mut()) {
                        if src_pixel.0[3] == 0 {
                            dest_pixel.0 = [255];
                            continue;
                        }
                        let luminance = src_pixel.to_luma().0[0];
                        dest_pixel.0 = [255 - luminance];
                    }

                    print_gray_image(&gray_image);

                    let result = tess::process([gray_image]).pop().unwrap();
                    println!("{result}");
                    // std::thread::sleep(Duration::from_secs(1));
                }
                _ => {
                    break;
                }
            }
        }
    }
}

fn crop_image(image: &RgbaImage) -> RgbaImage {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..image.height() {
        for x in 0..image.width() {
            let pixel = image.get_pixel(x, y);
            if pixel.0[3] > 0 {
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
            return RgbaImage::new(0, 0);
        }
        Some((x1, y1, x2, y2)) => {
            let mut new_image = RgbaImage::new(x2 + 1 - x1, y2 + 1 - y1);
            for (new_y, y) in (y1..=y2).enumerate() {
                for (new_x, x) in (x1..=x2).enumerate() {
                    new_image.put_pixel(new_x as _, new_y as _, image.get_pixel(x, y).clone());
                }
            }
            return new_image;
        }
    }
}
