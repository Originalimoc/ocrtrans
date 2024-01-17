use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba, RgbaImage, Luma, Pixel, GrayImage};
use imageproc::region_labelling::{connected_components, Connectivity};

pub fn remove_big_pixel_block(source_img: &DynamicImage, threshold: i32) -> RgbaImage {
	const MASK_BIT: u8 = 255; // for viewing when debug
	let img_is_white_bg = is_white_background(source_img);

	let background_pixel = if img_is_white_bg {
		Rgba([255, 255, 255, 255])
	} else {
		Rgba([0, 0, 0, 255])
	};

	let components_img = connected_components(source_img, Connectivity::Four, background_pixel);
	let (width, height) = components_img.dimensions();

	let mut max_lable = 0;
	for pixel in components_img.pixels() {
		let current_lable = (pixel.0)[0];
		max_lable = std::cmp::max(max_lable, current_lable);
	}
	let mut connected_region_list = vec![0; (max_lable + 1) as usize];
	for pixel in components_img.enumerate_pixels() {
		let current_lable = (pixel.2.0)[0];
		connected_region_list[current_lable as usize] += 1;
	}
	let mut big_chunk_lable = std::collections::VecDeque::new();
	connected_region_list.iter().enumerate().for_each(|(i, count)| {
		if count > &threshold {
			// log::debug!("lable {}: count: {}", i, count);
			big_chunk_lable.push_back(i as u32)
		}
	});
	big_chunk_lable.pop_front(); // 0 is background

	let mut mask_img: GrayImage = ImageBuffer::new(width, height);

	big_chunk_lable.iter().for_each(|lable| {
		for pixel in components_img.enumerate_pixels() {
			let current_lable = (pixel.2.0)[0];
			if lable == &current_lable {
				mask_img.put_pixel(pixel.0, pixel.1, Luma([MASK_BIT]))
			}
		}
	});

	mask_and_fill(source_img, &mask_img, MASK_BIT, background_pixel)
}

/// If mask_img pixel is filler_mask_bit, then source image will be filled with fill_pixel at that pixel.
/// # Panics
/// If source_img dimension is not the same as mask_img
fn mask_and_fill(source_img: &DynamicImage, mask_img: &GrayImage, filler_mask_bit: u8, fill_pixel: Rgba<u8>) -> RgbaImage {
	let (src_width, src_height) = source_img.dimensions();
	let (mask_width, mask_height) = mask_img.dimensions();
	if !(src_width == mask_width && src_height == mask_height) {
		panic!("mask_removal: both input img are not same dimensions");
	}
	
	// Create an ImageBuffer for the filtered image
	let mut filtered_img: RgbaImage = ImageBuffer::new(src_width, src_height);

	// Use Rayon to process the pixels in parallel
	filtered_img.enumerate_pixels_mut().for_each(|(x, y, pixel)| {
		let mask_pixel = mask_img.get_pixel(x, y).0[0]; // Assuming GrayImage has one channel

		if mask_pixel == filler_mask_bit {
			*pixel = fill_pixel;
		} else {
			let src_pixel = source_img.get_pixel(x, y).to_rgba();
			*pixel = src_pixel;
		}
	});

	filtered_img
}

// Filter only certain value pixels based on a predicate function
pub fn filter_pixels<F>(img: &DynamicImage, filler_pixel: Rgba<u8>, predicate: F) -> RgbaImage
where
	F: Fn(Rgba<u8>) -> bool,
{
	let (width, height) = img.dimensions();
	let mut filtered_img = ImageBuffer::new(width, height);

	for (x, y, og_pixel) in img.pixels() {
		if predicate(og_pixel) {
			filtered_img.put_pixel(x, y, og_pixel);
		} else {
			// Set unwanted pixels to fillter pixel
			filtered_img.put_pixel(x, y, filler_pixel);
		}
	}

	filtered_img
}

// Invert colors of the image
#[allow(dead_code)]
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

pub fn is_white_background(img: &DynamicImage) -> bool {
	let (width, height) = img.dimensions();
	let mut edge_pixel_count = 0usize;
	let mut edge_intensity_sum = 0u32;

	// Check the pixels along the edges of the image to determine the background color
	for x in 0..width {
		// Top edge
		edge_intensity_sum += img.get_pixel(x, 0).to_luma().channels()[0] as u32;
		// Bottom edge
		edge_intensity_sum += img.get_pixel(x, height - 1).to_luma().channels()[0] as u32;
		edge_pixel_count += 2;
	}
	for y in 1..(height - 1) {
		// Left edge
		edge_intensity_sum += img.get_pixel(0, y).to_luma().channels()[0] as u32;
		// Right edge
		edge_intensity_sum += img.get_pixel(width - 1, y).to_luma().channels()[0] as u32;
		edge_pixel_count += 2;
	}

	// Calculate the average intensity of the edge pixels
	let edge_avg_intensity = edge_intensity_sum / edge_pixel_count as u32;

	// Assuming that the text is either black or white, decide if the background is white
	// Here, we assume that if the average intensity is greater than 127 (midpoint of 0-255),
	// it's likely that the background is white
	edge_avg_intensity > 127
}
