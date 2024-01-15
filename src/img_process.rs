use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba, RgbaImage};

// Filter only certain value pixels based on a predicate function
pub fn filter_pixels<F>(img: &DynamicImage, predicate: F) -> RgbaImage
where
    F: Fn(Rgba<u8>) -> bool,
{
    let (width, height) = img.dimensions();
    let mut filtered_img = ImageBuffer::new(width, height);

    for (x, y, pixel) in img.pixels() {
        if predicate(pixel) {
            filtered_img.put_pixel(x, y, pixel);
        } else {
            // Set unwanted pixels to transparent or a background color
            filtered_img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
        }
    }

    filtered_img
}

// Invert colors of the image
pub fn invert_colors(img: &DynamicImage) -> RgbaImage {
    let (width, height) = img.dimensions();
    let mut filtered_img = ImageBuffer::new(width, height);
    img.to_rgba8().enumerate_pixels().for_each(|(x, y, pixel)| {
        let inverted_pixel = Rgba([
            255 - pixel[0],
            255 - pixel[1],
            255 - pixel[2],
            pixel[3],
        ]);
        filtered_img.put_pixel(x, y, inverted_pixel);
    });
    filtered_img
}
