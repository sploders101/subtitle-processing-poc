use image::{Rgb, Rgba};

pub fn print_rgba_image(image: &image::ImageBuffer<Rgba<u8>, Vec<u8>>) {
    let mut pix_buf: Vec<u8> = Vec::new();
    for pixel in image.pixels() {
        pix_buf.extend_from_slice(&pixel.0);
    }

    let encoder = sixel::encoder::Encoder::new().unwrap();
    encoder
        .encode_bytes(
            sixel::encoder::QuickFrameBuilder::new()
                .width(image.width() as _)
                .height(image.height() as _)
                .format(sixel_sys::PixelFormat::RGBA8888)
                .pixels(pix_buf),
        )
        .unwrap();
}

pub fn print_rgb_image(image: &image::ImageBuffer<Rgb<u8>, Vec<u8>>) {
    let mut pix_buf: Vec<u8> = Vec::new();
    for pixel in image.pixels() {
        pix_buf.extend_from_slice(&pixel.0);
    }

    let encoder = sixel::encoder::Encoder::new().unwrap();
    encoder
        .encode_bytes(
            sixel::encoder::QuickFrameBuilder::new()
                .width(image.width() as _)
                .height(image.height() as _)
                .format(sixel_sys::PixelFormat::RGB888)
                .pixels(pix_buf),
        )
        .unwrap();
}

pub fn print_gray_image(image: &image::GrayImage) {
    let mut pix_buf: Vec<u8> = Vec::new();
    for pixel in image.pixels() {
        pix_buf.extend_from_slice(&pixel.0);
    }

    let encoder = sixel::encoder::Encoder::new().unwrap();
    encoder
        .encode_bytes(
            sixel::encoder::QuickFrameBuilder::new()
                .width(image.width() as _)
                .height(image.height() as _)
                .format(sixel_sys::PixelFormat::G8)
                .pixels(pix_buf),
        )
        .unwrap();
}
