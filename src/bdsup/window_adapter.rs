pub struct ImageWindow<'a> {
    image: &'a mut image::GrayAlphaImage,
    x_cursor: u32,
    y_cursor: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    crop_origin: Option<(u32, u32)>,
}
impl<'a> ImageWindow<'a> {
    pub fn new(image: &'a mut image::GrayAlphaImage) -> Self {
        return Self {
            x_cursor: 0,
            y_cursor: 0,
            x: 0,
            y: 0,
            width: image.width(),
            height: image.height(),
            image,
            crop_origin: None,
        };
    }
    pub fn with_window(
        image: &'a mut image::GrayAlphaImage,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> ImageWindow<'a> {
        return Self {
            image,
            x_cursor: 0,
            y_cursor: 0,
            x,
            y,
            width,
            height,
            crop_origin: None,
        };
    }
    pub fn with_window_cropped(
        image: &'a mut image::GrayAlphaImage,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        crop_x: u32,
        crop_y: u32,
    ) -> Self {
        return Self {
            image,
            x_cursor: 0,
            y_cursor: 0,
            x,
            y,
            width,
            height,
            crop_origin: Some((crop_x, crop_y)),
        };
    }
    pub fn get_width(&self) -> u32 {
        return self.width;
    }
    pub fn get_height(&self) -> u32 {
        return self.height;
    }
    pub fn put_pixel(&mut self, mut x: u32, mut y: u32, pixel: image::LumaA<u8>) {
        if let Some((crop_x, crop_y)) = self.crop_origin {
            if x < crop_x || y < crop_y {
                return;
            }
            x = x - crop_x;
            y = y - crop_y;
        }
        if x >= self.width || y >= self.height {
            // If we're putting a pixel out of bounds, something's wrong with the input data,
            // which we don't really have control over, so just forget it.
            return;
        }
        x = x + self.x;
        y = y + self.y;
        if x >= self.image.width() || y >= self.image.height() {
            return;
        }
        if pixel.0[1] != 0 {
            self.image.put_pixel(x, y, pixel);
        }
    }
    pub fn push_pixel(&mut self, pixel: image::LumaA<u8>) {
        self.put_pixel(self.x_cursor, self.y_cursor, pixel);
        self.x_cursor += 1;
    }
    pub fn end_line(&mut self) {
        self.x_cursor = 0;
        self.y_cursor += 1;
    }
}
