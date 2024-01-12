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

const WM_UPDATE_TEXT: u32 = WM_USER + 1;
const BG_COLOR: u32 = 0x00101010;
const BG_ALPHA: u8 = 200;

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
pub fn create_window(message_channel: Receiver<String>, hwnd_return: Sender<UpdateHandle>) {
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

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TRANSPARENT,
            class_name,
            None,
            WS_POPUP,
            0,
            0,
            2560,
            80,
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
                    right: 1920,
                    bottom: 60,
                };
                let mut text_rect = *window_rect;
                if GetWindowRect(hwnd, window_rect).is_ok() {
                    text_rect = *window_rect;
                    text_rect.left = (*window_rect).left + 30;
                    text_rect.right = (*window_rect).right - 30;
                }
                let text_rect: *mut RECT = &mut text_rect;

                // let bitmap_width = (*window_rect).right - (*window_rect).left;
                // let bitmap_height = (*window_rect).bottom - (*window_rect).top;
                // let cbitmap = CreateCompatibleBitmap(hdc_window, bitmap_width, bitmap_height);

                let data_channel_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Receiver<_>;
                let data: String = match (*data_channel_ptr).try_recv() {
					Ok(data) => data,
					Err(e) => {
						eprintln!("windows::(*data_channel_ptr).try_recv(): {}", e);
						"No data".to_string()
					}
				};
                let mut display_text = Vec::from(HSTRING::from(data).as_wide());
                let display_text = &mut display_text[..];

                let bk_color = COLORREF(BG_COLOR);
                let brush = CreateSolidBrush(bk_color);
                FillRect(hdc_window, &ps.rcPaint, brush); // Fill the background

                SelectObject(hdc_window, h_font);
                SetTextColor(hdc_window, COLORREF(0x00FFFFFF));
                SetBkMode(hdc_window, TRANSPARENT);

                DrawTextW(hdc_window, display_text, text_rect, DT_SINGLELINE | DT_VCENTER | DT_RIGHT);

                // // Transfer the memory DC to the window's DC
                // let blend_function = BLENDFUNCTION {
                //     BlendOp: AC_SRC_OVER as u8,
                //     BlendFlags: 0,
                //     SourceConstantAlpha: BG_ALPHA, // Set the overall window opacity
                //     AlphaFormat: AC_SRC_ALPHA as u8,
                // };
                // let window_size = SIZE {
                //     cx: (*window_rect).right - (*window_rect).left, // Width of the window
                //     cy: (*window_rect).bottom - (*window_rect).top, // Height of the window
                // };
                // let result = UpdateLayeredWindow(
                //     hwnd,
                //     hdc_window,
                //     None, // Position can be None if you're updating the whole window
                //     Some(&window_size as *const SIZE),
                //     hdc_memory,
                //     Some(&POINT::default() as *const POINT), // Source position within the memory DC
                //     None, // Transparent color key (not used here)
                //     Some(&blend_function as *const BLENDFUNCTION),
                //     ULW_ALPHA,
                // );
                // if result.is_err() {
                //     // Handle the error
                //     dbg!(result);
                // }
                // DeleteObject(cbitmap);
                DeleteObject(h_font);
                DeleteObject(brush);
                EndPaint(hwnd, &ps);
                LRESULT(0)
            },
            WM_UPDATE_TEXT => {
                InvalidateRect(hwnd, None, true); // Invalidate the window to trigger a redraw
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
