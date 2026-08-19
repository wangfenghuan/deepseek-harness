//! macOS menu-bar (tray) icons that adapt to the system appearance: a light
//! theme shows the dark logo and a dark theme shows the light logo, matching
//! how the menu bar itself inverts. Windows keeps the default window icon.

use tauri::image::Image;
use tauri::Theme;

/// Menu-bar icons are ~22 pt tall; the committed sources are 640×640, so we
/// downscale to a sane size once at startup.
const MENU_BAR_SIZE: u32 = 32;

/// Build the menu-bar icon for the given system theme.
pub fn icon_for_theme(theme: Theme) -> Image<'static> {
    let bytes: &'static [u8] = match theme {
        // Dark menu bar (system dark appearance) needs a light glyph.
        Theme::Dark => include_bytes!("../../assets/deepseek-wite.png"),
        // Light menu bar needs a dark glyph.
        _ => include_bytes!("../../assets/deepseek-black.png"),
    };
    let decoded = image::load_from_memory(bytes).expect("embedded tray PNG must decode");
    let resized =
        decoded.resize(MENU_BAR_SIZE, MENU_BAR_SIZE, image::imageops::FilterType::Lanczos3);
    let rgba = resized.into_rgba8();
    Image::new_owned(rgba.into_raw(), MENU_BAR_SIZE, MENU_BAR_SIZE)
}
