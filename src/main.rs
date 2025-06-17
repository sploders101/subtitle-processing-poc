//! This is a proof-of-concept for extracting vobsub subtitles from an MKV file.
//! It makes use of some private functions from the vobsub crate, and requires a
//! modified copy to export them.
//!
//! This is primarily created as a testing ground for integrating subtitle extraction
//! into mediacorral. The current version really only works for vobsub, and converts
//! the vobsub images into sixel images, printing them to the terminal.

use image::{GrayImage, Pixel};
use matroska_demuxer::*;
use sixel::{print_gray_image, print_rgba_image};
use std::{fs::File, time::Duration};

mod sixel;
mod tess;
mod vobs;

fn main() {
    let file = File::open("test.mkv").unwrap();
    let mut mkv = MatroskaFile::open(file).unwrap();
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
    let vobs_idx = video_track.codec_private().map(vobs::parse_idx);

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
                    let result = vobs::parse_frame(idx, &frame.data);
                    print_rgba_image(&result);
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
                    std::thread::sleep(Duration::from_secs(1));
                }
                _ => {
                    break;
                }
            }
        }
    }
}
