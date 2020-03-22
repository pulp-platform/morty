//! Static files bundled with documentation output.

pub static LIGHT: &str = include_str!("static/light.css");
/// The file contents of the main `rustdoc.css` file, responsible for the core layout of the page.
pub static RUSTDOC_CSS: &str = include_str!("static/rustdoc.css");

/// Files related to the Fira Sans font.
pub mod fira_sans {
    /// The file `FiraSans-Regular.woff`, the Regular variant of the Fira Sans font.
    pub static REGULAR: &[u8] = include_bytes!("static/FiraSans-Regular.woff");

    /// The file `FiraSans-Medium.woff`, the Medium variant of the Fira Sans font.
    pub static MEDIUM: &[u8] = include_bytes!("static/FiraSans-Medium.woff");
}

/// Files related to the Source Serif Pro font.
pub mod source_serif_pro {
    /// The file `SourceSerifPro-Regular.ttf.woff`, the Regular variant of the Source Serif Pro
    /// font.
    pub static REGULAR: &[u8] = include_bytes!("static/SourceSerifPro-Regular.ttf.woff");

    /// The file `SourceSerifPro-Bold.ttf.woff`, the Bold variant of the Source Serif Pro font.
    pub static BOLD: &[u8] = include_bytes!("static/SourceSerifPro-Bold.ttf.woff");

    /// The file `SourceSerifPro-It.ttf.woff`, the Italic variant of the Source Serif Pro font.
    pub static ITALIC: &[u8] = include_bytes!("static/SourceSerifPro-It.ttf.woff");
}

/// Files related to the Source Code Pro font.
pub mod source_code_pro {
    /// The file `SourceCodePro-Regular.woff`, the Regular variant of the Source Code Pro font.
    pub static REGULAR: &[u8] = include_bytes!("static/SourceCodePro-Regular.woff");

    /// The file `SourceCodePro-Semibold.woff`, the Semibold variant of the Source Code Pro font.
    pub static SEMIBOLD: &[u8] = include_bytes!("static/SourceCodePro-Semibold.woff");
}
