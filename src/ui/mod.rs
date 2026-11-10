pub mod i18n;
pub mod main_window;
pub mod theme;

pub use i18n::{t, t_locale, Locale};
pub use main_window::MainWindow;
pub use theme::{LayoutSettings, ReadingMode, ReadingWidth};
