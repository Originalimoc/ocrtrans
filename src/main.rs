mod openaiapi;
mod hotkey;
mod translator;
use translator::TranslateRequest;
mod overlay;
use overlay::{create_window, UpdateHandle, WindowChannelMessage};

use std::{str::FromStr, time::Duration};
use std::io::Write;
use tokio::task::spawn_blocking;
use std::time::Instant;
pub use openssl;

use tesseract::Tesseract;
use screenshots::Screen;
use livesplit_hotkey::{Hook, Hotkey, Modifiers, KeyCode};
use notify_rust::Notification;
use clap::Parser;
use anyhow::{anyhow, Result};

const EMPTY_STRING_SIGNAL: &str = "		  			  			   ";

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
	/// percent of screen coordinates in horizontal then vertical order, ex. (0, 0.166, 0.75, 0.967) gives you bottom left region
	#[arg(short, long)]
	screen_region: String,

	/// OpenAI API endpoint, need /v1/chat/completions suffix
	#[arg(long, default_value = "http://172.22.22.172:8788/v1/chat/completions")]
	api_endpoint: String,

	/// OpenAI API key, optional if using non-official services
	#[arg(long)]
	api_key: Option<String>,

	/// Source translate language
	#[arg(long, default_value = "Japanese")]
	src_lang: String,

	/// Target translate language
	#[arg(long, default_value = "English")]
	target_lang: String,

	/// Default key to trigger a screen translation
	#[arg(long, default_value = "F6")]
	keyboard_shortcut: String,

	/// output rate will be limit to 1000/word_per_sec ms per word if output too fast
	#[arg(long, default_value_t = 10)]
	word_per_sec: u32
}

#[tokio::main]
async fn main() -> Result<()> {
	let args = Args::parse();
	let (screen_region,
		api_endpoint,
		api_key,
		src_lang,
		target_lang,
		word_per_sec,
		keyboard_shortcut) = (
			args.screen_region,
			args.api_endpoint,
			args.api_key,
			args.src_lang,
			args.target_lang,
			args.word_per_sec,
			args.keyboard_shortcut
		);

	let (ocr_channel_tx, ocr_channel_rx) = std::sync::mpsc::sync_channel(10);

	let xinput_hotkey_thread = {
		let src_lang = src_lang.clone();
		let screen_region = screen_region.clone();
		let ocr_channel_tx = ocr_channel_tx.clone();
		std::thread::spawn(|| hotkey::controller_combo_listener(move || screenshot_and_ocr(&src_lang, &screen_region, ocr_channel_tx.clone())))
	};

	
	let Ok(key_code) = KeyCode::from_str(&keyboard_shortcut) else {
		println!("Keyboard key \"{}\" not supported, view complete key list here: https://docs.rs/livesplit-hotkey/latest/src/livesplit_hotkey/key_code.rs.html#1788-2035", keyboard_shortcut);
		return Ok(());
	};
	let hotkey = Hotkey { key_code , modifiers: Modifiers::empty() };
	let Ok(hotkeyhook) = Hook::new() else {
		println!("Keyboard hotkey init failed");
		return Ok(());
	};
	{
		let src_lang = src_lang.clone();
		if hotkeyhook.register(hotkey, move || screenshot_and_ocr(&src_lang, &screen_region, ocr_channel_tx.clone())).is_err() {
			eprintln!("Keyboard hotkey init failed");
			return Ok(());
		}
	}
	
	let (result_display_tx, result_display_rx) = std::sync::mpsc::channel();
	let (window_handle_tx, window_handle_rx) = std::sync::mpsc::channel();
	std::thread::spawn(move || create_window(result_display_rx, window_handle_tx));
	let Ok(window_refresh) = window_handle_rx.recv() else {
		eprintln!("Window handle retriving error");
		return Ok(());
	};
	let buffered_display_in_tx_tty;
	let buffered_display_in_tx_clearer_2;
	{
		let api_endpoint = api_endpoint.clone();
		let api_key = api_key.clone();
		let src_lang = src_lang.clone();
		let target_lang = target_lang.clone();
		let (buffered_display_in_tx, buffered_display_in_rx) = std::sync::mpsc::channel();
		let (buffered_display_out_tx, buffered_display_out_rx) = std::sync::mpsc::channel();
		let buffered_display_in_tx_clearer_1 = buffered_display_in_tx.clone();
		buffered_display_in_tx_clearer_2 = buffered_display_in_tx.clone();
		buffered_display_in_tx_tty = buffered_display_in_tx.clone();
		let _ = new_display_buffer(buffered_display_in_rx, buffered_display_out_tx, 255, Duration::from_millis((1000.0 / word_per_sec as f64) as u64));
		spawn_blocking(move || {
			async_window_text_update(window_refresh, result_display_tx, buffered_display_out_rx)
		});
		std::thread::spawn(move || { // spawn the shortcut key listener(by ocr_channel_rx)
			let rt: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
				.enable_all()
				.build()
				.expect("Failed to create Tokio runtime");

			rt.block_on(async {
				while let Ok(ocr_text) = ocr_channel_rx.recv() {
					let translation_request = TranslateRequest::new(&ocr_text, &src_lang, &target_lang);
					let _ = buffered_display_in_tx_clearer_1.send(String::from(EMPTY_STRING_SIGNAL));
					if let Ok(result) = translator::translate_openai(
						&api_endpoint,
						&api_key,
						&translation_request,
						None,
						Some(buffered_display_in_tx.clone())
					).await {
						println!("{} Output> \n{}", target_lang, result);
						let _ = Notification::new()
							.summary("Translation result")
							.body(&result)
							.show();
					};
				}
			});
		});
	}

	std::thread::sleep(std::time::Duration::from_millis(100));
	if xinput_hotkey_thread.is_finished() {
		eprintln!("Controller hotkey init failed");
		std::process::exit(0);
	}

	println!("\nInit complete, press {} or D-Pad Right + Select(-) to trigger translation.\n", keyboard_shortcut);
	loop {
		let mut user_message = String::new();
		println!("{} Input> ", src_lang);
		if std::io::stdin().read_line(&mut user_message).is_err() {
			continue;
		};
		let user_message = user_message.trim().to_string();

		println!("Streaming {} output> \n", target_lang);
		let (streaming_output_tx, streaming_output_rx) = tokio::sync::mpsc::channel(10);
		let translation_request = TranslateRequest::new(&user_message, &src_lang, &target_lang);
		let _ = buffered_display_in_tx_clearer_2.send(String::from(EMPTY_STRING_SIGNAL));
		let (_, Ok(_)) = tokio::join!(async_display_print(streaming_output_rx, false), translator::translate_openai(
			&api_endpoint,
			&api_key,
			&translation_request,
			Some(streaming_output_tx),
			Some(buffered_display_in_tx_tty.clone()),
		)) else { continue };
		println!("\n\n/Streaming {} output done\n", target_lang);
	}
}

fn screenshot_and_ocr(lang: &str, screen_region: &str, output_channel: std::sync::mpsc::SyncSender<String>) {
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
	image.save("last_ocr_screenshot.png").unwrap();
	let ocr_start_time = Instant::now();
	let Ok(mut tess) = Tesseract::new(None, Some(lang)) else {
		eprintln!("Could not initialize tesseract, missing {}.traineddata", lang);
		return;
	};
	tess = tess.set_image("last_ocr_screenshot.png").unwrap();
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

fn new_display_buffer(
	input_channel: std::sync::mpsc::Receiver<String>,
	output_channel: std::sync::mpsc::Sender<String>,
	max_len: usize,
	min_interval: Duration, // The minimum time interval between consecutive sends
) -> std::thread::JoinHandle<()> {
	std::thread::spawn(move || {
		let mut buffer: String = String::new();
		let mut last_send_time = Instant::now(); // Record the last send time

		loop {
			match input_channel.recv() {
				Ok(input) => {
					if input == EMPTY_STRING_SIGNAL {
						buffer.clear();
						let _ = output_channel.send("".to_string()); 
						continue;
					};
					buffer.push_str(&input);
					while buffer.chars().count() > max_len {
						buffer.remove(0);
					}

					// Calculate the elapsed time since the last send
					let elapsed_time = last_send_time.elapsed();

					// Check if the elapsed time is less than the minimum interval
					if elapsed_time < min_interval {
						// Sleep for the remaining time
						std::thread::sleep(min_interval - elapsed_time);
					}

					// Send the buffer to the output channel
					if output_channel.send(buffer.clone()).is_err() {
						eprintln!("output channel closed");
						break;
					}

					// Update the last send time
					last_send_time = Instant::now();
				}
				Err(_) => {
					eprintln!("input channel closed");
					break;
				}
			}
		}
	})
}

async fn async_display_print(mut content_channel: tokio::sync::mpsc::Receiver<String>, newline: bool) {
	while let Some(content) = content_channel.recv().await {
		if newline {
			println!("{}", content);
		} else {
			print!("{}", content);
			let _ = std::io::stdout().flush();
		}
	}
}

/// WARNING: Blocking function until input_channel is closed
fn async_window_text_update(window_refresh: UpdateHandle, window_content_output: std::sync::mpsc::Sender<WindowChannelMessage>, input_channel: std::sync::mpsc::Receiver<String>) {
	while let Ok(content) = input_channel.recv() {
		let _ = window_content_output.send(WindowChannelMessage {
			text: content,
			screen_dimension: None
		});
		let _ = window_refresh.update_window();
	}
}
