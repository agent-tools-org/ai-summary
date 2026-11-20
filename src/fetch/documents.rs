//! Detect and extract text from bounded PDF and DOCX bodies.
//! DOCX extraction decodes XML entities and limits inflated document bytes.

use crate::fetch_utils::collapse_ws;
use std::io::{Cursor, Read};

const XML_BYTES: u64 = 10_000_000;

pub(super) enum Format {
    Pdf,
    Docx,
}

impl Format {
    pub fn detect(media: &str, path: &str) -> Option<Self> {
        let path = path.to_ascii_lowercase();
        if media.contains("application/pdf") || path.ends_with(".pdf") {
            Some(Self::Pdf)
        } else if media.contains("wordprocessingml.document") || path.ends_with(".docx") {
            Some(Self::Docx)
        } else {
            None
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Docx => "docx",
        }
    }

    pub fn media_type(&self) -> &'static str {
        match self {
            Self::Pdf => "application/pdf",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        }
    }

    pub fn extract(&self, bytes: &[u8]) -> Result<String, String> {
        let text = match self {
            Self::Pdf => pdf_extract::extract_text_from_mem(bytes)
                .map_err(|_| "PDF extraction failed".to_owned())?,
            Self::Docx => extract_docx(bytes)?,
        };
        Ok(collapse_ws(text.trim()))
    }
}

fn extract_docx(bytes: &[u8]) -> Result<String, String> {
    let invalid = |_| "DOCX extraction failed".to_owned();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(invalid)?;
    let xml = archive.by_name("word/document.xml").map_err(invalid)?;
    if xml.size() > XML_BYTES {
        return Err("inflated DOCX exceeds document budget".into());
    }
    let mut text = String::new();
    xml.take(XML_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|_| "invalid DOCX XML")?;
    if text.len() as u64 > XML_BYTES {
        return Err("inflated DOCX exceeds document budget".into());
    }
    // Parse text and entities with the same parser used for fetched HTML. Word text
    // elements are inline, so paragraph boundaries are inserted explicitly first.
    let text = text
        .replace("</w:p>", "</w:p> ")
        .replace("<w:tab/>", " ")
        .replace("<w:br/>", " ");
    Ok(crate::fetch_utils::strip_html(&text))
}
