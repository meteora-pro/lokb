pub mod epub;
pub mod html;
pub mod markdown;
pub mod pdf;
pub mod zim;

pub use epub::extract_epub;
pub use html::HtmlToMarkdown;
pub use markdown::MarkdownPassthrough;
pub use pdf::extract_pdf;
pub use zim::ZimReader;
