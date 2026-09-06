use iced::{Task, window};

#[cfg(any(target_os = "macos", test))]
const TITLEBAR_HEIGHT: f64 = 44.0;

#[cfg(any(target_os = "macos", test))]
fn centered_origin(container_height: f64, item_height: f64) -> f64 {
    (container_height - item_height) / 2.0
}

pub(crate) fn prepare_window(id: window::Id) -> Task<()> {
    #[cfg(target_os = "macos")]
    {
        return window::run(id, macos::center_traffic_lights);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = id;
        Task::none()
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{TITLEBAR_HEIGHT, centered_origin};
    use iced::window::raw_window_handle::RawWindowHandle;
    use objc2_app_kit::{NSView, NSWindowButton};

    pub(super) fn center_traffic_lights(window: &dyn iced::window::Window) {
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };

        // SAFETY: Iced keeps this AppKit view alive while `window::run` executes on
        // the window event thread. The pointer is used only for this callback.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let Some(window) = view.window() else {
            return;
        };
        let Some(close) = window.standardWindowButton(NSWindowButton::CloseButton) else {
            return;
        };

        // SAFETY: AppKit owns the standard titlebar hierarchy for the lifetime of
        // the native buttons. Missing or changed hierarchy levels are handled.
        let Some(titlebar) = (unsafe { close.superview().and_then(|view| view.superview()) })
        else {
            return;
        };
        let mut titlebar_frame = titlebar.frame();
        titlebar_frame.size.height = TITLEBAR_HEIGHT;
        titlebar_frame.origin.y = window.frame().size.height - TITLEBAR_HEIGHT;
        titlebar.setFrame(titlebar_frame);

        for kind in [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ] {
            let Some(button) = window.standardWindowButton(kind) else {
                continue;
            };
            let mut frame = button.frame();
            frame.origin.y = centered_origin(TITLEBAR_HEIGHT, frame.size.height);
            button.setFrameOrigin(frame.origin);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TITLEBAR_HEIGHT, centered_origin};

    #[test]
    fn traffic_lights_are_centered_in_the_titlebar() {
        assert_eq!(centered_origin(TITLEBAR_HEIGHT, 14.0), 15.0);
    }
}
