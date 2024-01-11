mod openaiapi;
mod hotkey;
mod translator;
use translator::TranslateRequest;

use core::str::FromStr;
pub use openssl;

use tesseract::Tesseract;
use screenshots::Screen;
use livesplit_hotkey::{Hook, Hotkey, Modifiers, KeyCode};
use notify_rust::Notification;
use clap::Parser;
use anyhow::{anyhow, Result};
use tokio::sync::mpsc::{channel, Sender};

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
    keyboard_shortcut: String
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let (screen_region,
        api_endpoint,
        api_key,
        src_lang,
        target_lang,
        keyboard_shortcut) = (
            args.screen_region,
            args.api_endpoint,
            args.api_key,
            args.src_lang,
            args.target_lang,
            args.keyboard_shortcut
        );

    let (ocr_channel_tx, mut ocr_channel_rx) = channel(10);

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
    
    {
        let api_endpoint = api_endpoint.clone();
        let api_key = api_key.clone();
        let src_lang = src_lang.clone();
        let target_lang = target_lang.clone();
        tokio::spawn(async move {
            while let Some(ocr_text) = ocr_channel_rx.recv().await {
                if let Ok(result) = translator::translate_openai(
                    &api_endpoint,
                    &api_key,
                    &TranslateRequest::new(&ocr_text, &src_lang, &target_lang),
                ).await {
                    println!("{} Output> \n{}", target_lang, result);
                    let _ = Notification::new()
                        .summary("Translation result")
                        .body(&result)
                        .show();
                };
            }
        });
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    if xinput_hotkey_thread.is_finished() {
        eprintln!("Controller hotkey init failed");
        std::process::exit(0);
    }

    println!("\nInit complete, press {} or D-Pad Down + Right Stick to trigger translation.\n", keyboard_shortcut);
    loop {
        let mut user_message = String::new();
        println!("{} Input> ", src_lang);
        if std::io::stdin().read_line(&mut user_message).is_err() {
            continue;
        };
        let user_message = user_message.trim().to_string();

        let Ok(result) = translator::translate_openai(
            &api_endpoint,
            &api_key,
            &TranslateRequest::new(&user_message, &src_lang, &target_lang),
        ).await else { continue };
        println!("{} Output> \n{}", target_lang, result);
    }
}

fn screenshot_and_ocr(lang: &str, screen_region: &str, output_channel: Sender<String>) {
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
    let real_resoltion = ((screen.display_info.width as f64 / screen.display_info.scale_factor as f64) as u32, (screen.display_info.height as f64 / screen.display_info.scale_factor as f64) as u32);
    let Ok(ocr_screen_region) = convert_screen_region(real_resoltion, screen_region) else {
        eprintln!("Error: Screen region parsing failed");
        return;
    };
    let image = screen.capture_area_ignore_area_check(ocr_screen_region.0, ocr_screen_region.1, ocr_screen_region.2, ocr_screen_region.3).unwrap();
    image.save("last_ocr_screenshot.png").unwrap();
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
    println!("\nOCR get text:\n{}\n", ocr_output_text);
    let _ = output_channel.blocking_send(ocr_output_text);
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
