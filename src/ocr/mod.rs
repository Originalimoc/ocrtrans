use image::{imageops::crop_imm, DynamicImage, GenericImageView, GrayImage, ImageBuffer, Luma, Pixel};
use xcap::Monitor;
use anyhow::{anyhow, Result};
use reqwest::blocking::multipart;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct Response {
    extracted_text: String,
}

pub fn screenshot_and_ocr(screen_region: &str, output_channel: std::sync::mpsc::SyncSender<String>, ocr_api_endpoint: &str) {
	let screen = {
		let screens = Monitor::all().unwrap_or_default();
		if screens.is_empty() {
			println!("No screen detected");
			return;
		}
		if screens.len() >= 2 {
			println!("Multiple screens detected, only first screen will be used.");
		}
		screens
	};
	let screen = &screen[0];
	// println!("Capturing screen info: {screen:?}");
	let real_resoltion = (screen.width(), screen.height());
	let Ok(ocr_screen_region) = convert_screen_region(real_resoltion, screen_region) else {
		eprintln!("Error: Screen region parsing failed");
		return;
	};
	let image = screen.capture_image().unwrap();
	let image = crop_imm(&image, ocr_screen_region.0, ocr_screen_region.1, ocr_screen_region.2, ocr_screen_region.3).to_image();

	let image = DynamicImage::ImageRgba8(image);
	let (original_width, original_height) = image.dimensions();
	let scaling_factor = 1.0 / 3.0;
    let new_width = (original_width as f32 * scaling_factor).round() as u32;
    let new_height = (original_height as f32 * scaling_factor).round() as u32;
	let image = image.resize_exact(new_width, new_height, image::imageops::FilterType::Nearest);
	let image = image.adjust_contrast(25.0);
	let image = image.grayscale();
	let image = filter_pixels(&image, Luma([0]), |x| { x.0[0] > 220 });

	let _ = image.save("last_ocr_screenshot.png");
    let mut buffer = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut buffer), image::ImageFormat::Png).unwrap();

	let form_for_ocrserver = multipart::Form::new().part("image", multipart::Part::bytes(buffer).file_name("image.png"));
	let response = reqwest::blocking::Client::new()
		.post(ocr_api_endpoint)
		.multipart(form_for_ocrserver)
		.send().unwrap()
		.text().unwrap();
	let extracted_response: Response = serde_json::from_str(&response).unwrap();
	println!("OCR extracted text:\n{}", extracted_response.extracted_text);
	let _ = output_channel.send(extracted_response.extracted_text);
}

fn convert_screen_region(resolution: (u32, u32), target_region: &str) -> Result<(u32, u32, u32, u32)> {
	let target_region = parse_tuple_of_4f64(target_region)?;
	let target_region = [
		target_region.0,
		target_region.1,
		target_region.2,
		target_region.3
	];
	if target_region.iter().any(|tr| !(0.0..=1.0).contains(tr)) {
		return Err(anyhow!("Wrong screen capture region set 0x1"));
	}
	if target_region[1] < target_region[0] || target_region[3] < target_region[2] {
		return Err(anyhow!("Wrong screen capture region set 0x2"));
	}
	let (width_start, width_end, height_start, height_end) = (
		f64::from(resolution.0) * target_region[0],
		f64::from(resolution.0) * target_region[1],
		f64::from(resolution.1) * target_region[2],
		f64::from(resolution.1) * target_region[3],
	);
	Ok((
		width_start as u32,
		height_start as u32,
		(width_end - width_start).round() as u32,
		(height_end - height_start).round() as u32,
	))
}

fn parse_tuple_of_4f64(input: &str) -> Result<(f64, f64, f64, f64)> {
	let s = input.trim().trim_start_matches('(').trim_end_matches(')');
	let parts: Vec<&str> = s.split(',').collect();

	if parts.len() != 4 {
		return Err(anyhow!("Should input 4 elements should but get {}", input.len()));
	}

	let parsed_numbers: Result<Vec<f64>, _> = parts.iter().map(|&x| x.trim().parse::<f64>()).collect();

	match parsed_numbers {
		Ok(numbers) => Ok((numbers[0], numbers[1], numbers[2], numbers[3])),
		Err(_) => Err(anyhow!("Parsing failed")),
	}
}

fn filter_pixels<F>(img: &DynamicImage, filler_pixel: Luma<u8>, predicate: F) -> GrayImage
where
	F: Fn(&Luma<u8>) -> bool,
{
	let (width, height) = img.dimensions();
	let mut filtered_img = ImageBuffer::new(width, height);

	for (x, y, og_pixel) in img.pixels() {
		if predicate(&og_pixel.to_luma()) {
			filtered_img.put_pixel(x, y, og_pixel.to_luma());
		} else {
			// Set unwanted pixels to fillter pixel
			filtered_img.put_pixel(x, y, filler_pixel);
		}
	}

	filtered_img
}
