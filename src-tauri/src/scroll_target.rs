/// Selects the capture approach based on the active application identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollCaptureStrategy {
    BrowserPage,
    DocumentApp,
    DesktopStitch,
}

pub fn classify_scroll_target(app_name: &str, title: &str) -> ScrollCaptureStrategy {
    let app = app_name.to_ascii_lowercase();
    let title = title.to_ascii_lowercase();
    if ["google chrome", "chrome", "brave browser", "microsoft edge", "safari", "firefox"]
        .iter()
        .any(|name| app.contains(name))
        || title.contains("google chrome")
        || title.contains("microsoft edge")
    {
        ScrollCaptureStrategy::BrowserPage
    } else if ["microsoft word", "pages", "libreoffice", "preview", "acrobat"]
        .iter()
        .any(|name| app.contains(name) || title.contains(name))
    {
        ScrollCaptureStrategy::DocumentApp
    } else {
        ScrollCaptureStrategy::DesktopStitch
    }
}
