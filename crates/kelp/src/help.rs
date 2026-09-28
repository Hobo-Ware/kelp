#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    GettingStarted,
    Shortcuts,
    Undo,
    Faq,
    Changelog,
    ReportBug,
}

impl Page {
    pub const IN_SETTINGS: [Page; 4] = [
        Page::GettingStarted,
        Page::Shortcuts,
        Page::Changelog,
        Page::ReportBug,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::GettingStarted => "Getting started",
            Page::Shortcuts => "Shortcuts",
            Page::Undo => "What undo covers",
            Page::Faq => "FAQ",
            Page::Changelog => "Changelog",
            Page::ReportBug => "Report a bug",
        }
    }

    pub fn url(self) -> &'static str {
        match self {
            Page::GettingStarted => "https://kelp.hoboware.dev/docs/getting-started.html",
            Page::Shortcuts => "https://kelp.hoboware.dev/docs/shortcuts.html",
            Page::Undo => "https://kelp.hoboware.dev/docs/undo.html",
            Page::Faq => "https://kelp.hoboware.dev/docs/faq.html",
            Page::Changelog => "https://kelp.hoboware.dev/changelog.html",
            Page::ReportBug => "https://github.com/Hobo-Ware/kelp/issues/new",
        }
    }

    pub fn open(self) -> std::io::Result<()> {
        crate::pulls_ui::open_url(self.url())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_pages_exist_for_every_link() {
        let site = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../site");
        for page in [
            Page::GettingStarted,
            Page::Shortcuts,
            Page::Undo,
            Page::Faq,
            Page::Changelog,
        ] {
            let path = page
                .url()
                .strip_prefix("https://kelp.hoboware.dev/")
                .expect("site url");
            assert!(site.join(path).is_file(), "{path} is missing from site/");
        }
    }
}
