use std::time::Instant;
use tesseract::Tesseract;
use screenshots::Screen;
use anyhow::{anyhow, Result};
mod img_process;

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

    let processing_img = image::open(format!("{}_og.png", tmp_filename)).expect("Failed to open image");
    let processing_img = processing_img.adjust_contrast(100.0);
    let processing_img = img_process::filter_pixels(&processing_img, |p| {
        p[0] == 255 && p[1] == 255 && p[2] == 255
    });
    let processing_img = img_process::invert_colors(&(processing_img.into()));
    processing_img.save(format!("{}_processed.png", tmp_filename)).unwrap();

	let ocr_start_time = Instant::now();
	let Ok(mut tess) = Tesseract::new(None, Some(lang)) else {
		eprintln!("Could not initialize tesseract, missing {}.traineddata", lang);
		return;
	};
	tess = tess.set_image(&format!("{}_processed.png", tmp_filename)).unwrap();
	let Ok(mut ocr_output_text) = tess.get_text() else {
		eprintln!("Could not perform OCR");
		return;
	};
	ocr_output_text = ocr_output_text.replace("\n\n", "").replace(' ', "");
	println!("\nOCR get text after {:.2}s:\n{}\n", ocr_start_time.elapsed().as_secs_f64(), ocr_output_text);
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
