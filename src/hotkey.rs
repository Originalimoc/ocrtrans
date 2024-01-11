use rusty_xinput as xi;
use std::time::Duration;

enum ControllerState {
    NotPressed,
    KeyA,
    KeyB,
    BothKeys,
}

/// std::thread::spawn(move || hotkey::controller_combo_listener(move || callback()))
pub fn controller_combo_listener(mut f: impl FnMut()) {
    let mut current_combo_state = ControllerState::NotPressed;
    let xi_handle = match xi::XInputHandle::load_default() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Init XInput error: {:?}", e);
            return;
        }
    };
    let mut err_print = false;
    loop {
        let xstate_get = match xi_handle.get_state_ex(0) {
            Ok(s) => {
                err_print = false;
                s
            }
            Err(e) => {
                if !err_print {
                    println!("Get XInput state failed: {:?}, retrying...", e);
                    err_print = true;
                }
                std::thread::sleep(Duration::from_millis(333));
                continue;
            }
        };
        let key_a_pressed = xstate_get.arrow_down();
        let key_b_pressed = xstate_get.right_thumb_button();
        current_combo_state = match current_combo_state {
            ControllerState::NotPressed => {
                if key_b_pressed {
                    ControllerState::KeyB
                } else if key_a_pressed {
                    ControllerState::KeyA
                } else {
                    ControllerState::NotPressed
                }
            }
            ControllerState::KeyB => {
                if key_a_pressed {
                    ControllerState::BothKeys
                } else if !key_b_pressed {
                    ControllerState::NotPressed
                } else {
                    ControllerState::KeyB
                }
            }
            ControllerState::KeyA => {
                if key_b_pressed {
                    ControllerState::BothKeys
                } else if !key_a_pressed {
                    ControllerState::NotPressed
                } else {
                    ControllerState::KeyA
                }
            }
            ControllerState::BothKeys => {
                if !key_b_pressed || !key_a_pressed {
                    f();
                    ControllerState::NotPressed
                } else {
                    ControllerState::BothKeys
                }
            }
        };
        std::thread::sleep(Duration::from_millis(3));
    }
}
