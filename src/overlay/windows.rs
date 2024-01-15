use std::sync::mpsc::{Receiver, Sender};
use anyhow::Result;
use windows::{
	core::*,
	Win32::{
		Foundation::*,
		Graphics::Gdi::*,
		System::LibraryLoader::*,
		UI::WindowsAndMessaging::*,
	},
};

use screenshots::Screen;

const WM_UPDATE_TEXT: u32 = WM_USER + 1;
const BG_COLOR: u32 = 0x00101010;
const BG_ALPHA: u8 = 200;
const OVERLAY_HEIGHT: i32 = 52; // (1600-1440) / 2 / 1.5, this targets 1.5x scale on a 2560x1600 screen while not overlay with 16:9 content

pub struct WindowChannelMessage {
	pub text: String,
	pub screen_dimension: Option<(u32, u32, f64)>,
}

pub struct UpdateHandle {
	inner: HWND
}

impl UpdateHandle {
	fn new(inner: HWND) -> Self {
		Self {
			inner
		}
	}
	pub fn update_window(&self) -> Result<()> {
		unsafe {
			PostMessageW(self.inner, WM_UPDATE_TEXT, WPARAM(0), LPARAM(0))?;
			Ok(())
		}
	}
}

/// create_window() launch example
/// ```
/// let (text_tx, text_rx) = sync_channel(10);
/// let (window_handle_tx, window_handle_rx) = sync_channel(10);
/// std::thread::spawn(move || create_window(text_rx, window_handle_tx));
/// let window_refresh = window_handle_rx.recv().unwrap();
/// ```
/// Then
/// ```
/// let _ = text_tx.send(result);
/// let _ = window_refresh.update_window();
/// ```
pub fn create_window(message_channel: Receiver<WindowChannelMessage>, hwnd_return: Sender<UpdateHandle>) {
	unsafe {
		let instance = GetModuleHandleW(None).unwrap().into();
		let class_name = w!("Main Window");

		let wc = WNDCLASSW {
			hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
			hInstance: instance,
			lpszClassName: class_name,
			style: CS_HREDRAW | CS_VREDRAW,
			lpfnWndProc: Some(window_proc),
			..Default::default()
		};

		RegisterClassW(&wc);

		let screen_width = get_screen_width_sf().0;
		let hwnd = CreateWindowExW(
			WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TRANSPARENT,
			class_name,
			None,
			WS_POPUP,
			0,
			0,
			screen_width,
			OVERLAY_HEIGHT,
			None,
			None,
			instance,
			None,
		);

		// Set the window as layered with 50% opacity (128 out of 255)
		SetLayeredWindowAttributes(hwnd, COLORREF(BG_COLOR), BG_ALPHA, LWA_ALPHA).unwrap();

		
		let window_data_ptr = Box::into_raw(Box::new(message_channel));
		SetWindowLongPtrW(hwnd, GWLP_USERDATA, window_data_ptr as isize);

		ShowWindow(hwnd, SW_SHOW);
		UpdateWindow(hwnd);

		if hwnd_return.send(UpdateHandle::new(hwnd)).is_err() {
			eprintln!("Can't return HWND handle");
			std::process::exit(0);
		};

		let mut msg = MSG::default();
		while GetMessageW(&mut msg, None, 0, 0).into() {
			TranslateMessage(&msg);
			DispatchMessageW(&msg);
		}
	}
}

extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
	unsafe {
		match message {
			WM_DESTROY => {
				PostQuitMessage(0);
				LRESULT(0)
			}
			WM_PAINT => {
				let font_size = 42;
				let h_font = CreateFontW(
					font_size,
					0,
					0,
					0,
					FW_NORMAL.0 as i32,
					false.into(),
					false.into(),
					false.into(),
					DEFAULT_CHARSET.0 as u32,
					OUT_OUTLINE_PRECIS.0 as u32,
					CLIP_DEFAULT_PRECIS.0 as u32,
					ANTIALIASED_QUALITY.0 as u32,
					VARIABLE_PITCH.0 as u32,
					w!("Adagio Sans"),
				);

				let mut ps = PAINTSTRUCT::default();
				let hdc_window = BeginPaint(hwnd, &mut ps);

				let window_rect: *mut RECT = &mut RECT {
					left: 0,
					top: 0,
					right: 1606,
					bottom: OVERLAY_HEIGHT,
				};
				let mut text_rect = *window_rect;
				if GetWindowRect(hwnd, window_rect).is_ok() {
					text_rect = *window_rect;
					text_rect.left = (*window_rect).left + 30;
					text_rect.right = (*window_rect).right - 30;
				}
				let text_rect: *mut RECT = &mut text_rect;

				let data_channel_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Receiver<WindowChannelMessage>;
				let display_text: String = match (*data_channel_ptr).try_recv() {
					Ok(data) => {
						if data.text.is_empty() {
							" ".to_string() // "" will cause Win32 to crash...?
						} else {
							data.text
						}
					},
					Err(_) => {
						// eprintln!("windows::(*data_channel_ptr).try_recv(): {}", e);
						"Waiting for input".to_string()
					}
				};
				let mut display_text = Vec::from(HSTRING::from(display_text).as_wide());
				let display_text = &mut display_text[..];

				let bk_color = COLORREF(BG_COLOR);
				let brush = CreateSolidBrush(bk_color);
				FillRect(hdc_window, &ps.rcPaint, brush); // Fill the background

				SelectObject(hdc_window, h_font);
				SetTextColor(hdc_window, COLORREF(0x00FFFFFF));
				SetBkMode(hdc_window, TRANSPARENT);

				DrawTextW(hdc_window, display_text, text_rect, DT_SINGLELINE | DT_VCENTER | DT_RIGHT);

				DeleteObject(h_font);
				DeleteObject(brush);
				EndPaint(hwnd, &ps);
				LRESULT(0)
			},
			WM_UPDATE_TEXT => {
				InvalidateRect(hwnd, None, true); // Invalidate the window to trigger a redraw
				LRESULT(0)
			},
			#[allow(unreachable_patterns)] // WM_SETTINGCHANGE and WM_WININICHANGE are both 26
			WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_WININICHANGE | WM_DPICHANGED => {
				let width = get_screen_width_sf().0;
				if SetWindowPos(hwnd, None, 0, 0, width, OVERLAY_HEIGHT, SWP_NOZORDER | SWP_NOMOVE).is_err() {
					LRESULT(-1)
				} else {
					LRESULT(0)
				}
			},
			_ => DefWindowProcW(hwnd, message, wparam, lparam),
		}
	}
}

// return 1706 and 150% if failed
fn get_screen_width_sf() -> (i32, f64) {
	let Ok(screens) = Screen::all() else {
		return (1706, 1.5); // 2560/1.5
	};
	if screens.is_empty() {
		return (1706, 1.5); // 2560/1.5
	};
	(screens[0].display_info.width as i32, screens[0].display_info.scale_factor as f64)
}
