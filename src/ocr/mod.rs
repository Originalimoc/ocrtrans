use std::time::Instant;
use image::{imageops::invert, DynamicImage, GrayImage, Luma, Rgba};
use tesseract::Tesseract;
use screenshots::Screen;
use anyhow::{anyhow, Result};
mod img_process;
use img_process::*;

const REGION_SIZE_THRESHOLD_H: i32 = 1500;
const REGION_SIZE_THRESHOLD_L: i32 = 0;
const MASK_BIT: u8 = 255; // for viewing when debug
const NON_MASK_BIT: u8 = 0; // for viewing when debug

pub fn screenshot_and_ocr(lang: &str, screen_region: &str, output_channel: std::sync::mpsc::SyncSender<String>, tmp_filename: &str) {
	let screen = {
		let screens = Screen::all().unwrap_or_default();
		if screens.is_empty() {
			println!("No screen detected");
			return;
		}
		if screens.len() >= 2 {
			println!("Multiple screens detected, only first screen will be used.");
		}
		screens[0]
	};
	// println!("Capturing screen info: {screen:?}");
	let real_resoltion = ((screen.display_info.width as f64 * screen.display_info.scale_factor as f64) as u32, (screen.display_info.height as f64 * screen.display_info.scale_factor as f64) as u32);
	let Ok(ocr_screen_region) = convert_screen_region(real_resoltion, screen_region) else {
		eprintln!("Error: Screen region parsing failed");
		return;
	};
	let image = screen.capture_area_ignore_area_check(ocr_screen_region.0, ocr_screen_region.1, ocr_screen_region.2, ocr_screen_region.3).unwrap();
	image.save(format!("{}_og.png", tmp_filename)).unwrap();

	let og_img = image::open(format!("{}_og.png", tmp_filename)).expect("Failed to open image");
	let mut high_contrast_img = og_img.adjust_contrast(69.0);

	let mut img_is_white_bg = is_white_background(&high_contrast_img);
	println!("\nOCR detected {} background, processing as black BG", if img_is_white_bg { "white" } else { "black" });

	if img_is_white_bg {
		invert(&mut high_contrast_img);
		img_is_white_bg = !img_is_white_bg;
	}

	let is_content_predicate = |p: &Rgba<u8>| {
        if img_is_white_bg {
            (p[0] as u16 + p[1] as u16 + p[1] as u16) < 255 * 3 - 80 // get black text for white bg
        } else {
            (p[0] as u16 + p[1] as u16 + p[1] as u16) > 80 // get white text for black bg
        }
	};
	let background_pixel = if img_is_white_bg {
		Rgba([255, 255, 255, 255])
	} else {
		Rgba([0, 0, 0, 255])
	};


	let processing_img = filter_pixels(&high_contrast_img, background_pixel, is_content_predicate);
	let processing_img = remove_big_tiny_pixel_block(&DynamicImage::ImageRgba8(processing_img), REGION_SIZE_THRESHOLD_H, REGION_SIZE_THRESHOLD_L);

	let text_mask_by_pi: GrayImage = rgba_to_mask(&processing_img, Luma([MASK_BIT]), Luma([NON_MASK_BIT]), is_content_predicate);
	text_mask_by_pi.save(format!("{}_result_img.png", tmp_filename)).unwrap();

    // let og_masked = mask_and_fill(&og_img, &text_mask_by_pi, NON_MASK_BIT, background_pixel);
	// let og_masked_and_normalized = if !img_is_white_bg {
	// 	normalize_brightness(&DynamicImage::ImageRgba8(og_masked.clone()))
	// } else {
	// 	og_masked.clone()
	// };
	// og_masked_and_normalized.save(format!("{}_og_masked_and_normalized.png", tmp_filename)).unwrap();
    // let colors = most_color(&og_masked_and_normalized, 10, 16);
    // let result_img = filter_pixels(&DynamicImage::ImageRgba8(og_masked), background_pixel, |pixel| {
    //     colors.iter().any(|color| same_rbga(&color.0, pixel, 16))
    // });
	// result_img.save(format!("{}_result_img.png", tmp_filename)).unwrap();

	let ocr_start_time = Instant::now();
	let Ok(mut tess) = Tesseract::new(None, Some(lang)) else {
		eprintln!("Could not initialize tesseract, missing {}.traineddata", lang);
		return;
	};
	tess = tess.set_image(&format!("{}_result_img.png", tmp_filename)).unwrap();
	let Ok(mut ocr_output_text) = tess.get_text() else {
		eprintln!("Could not perform OCR");
		return;
	};
	ocr_output_text = ocr_output_text.replace("\n\n", "").replace(' ', "");
	println!("OCR get text after {:.2}ms:\n{}\n", ocr_start_time.elapsed().as_millis(), ocr_output_text);
	let _ = output_channel.send(ocr_output_text);
}

fn convert_screen_region(resolution: (u32, u32), target_region: &str) -> Result<(i32, i32, u32, u32)> {
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
		width_start as i32,
		height_start as i32,
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
