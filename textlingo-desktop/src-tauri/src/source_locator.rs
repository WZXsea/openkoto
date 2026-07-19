use serde::{Deserialize, Serialize};

pub const SOURCE_LOCATOR_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceLocator {
    #[serde(default = "default_version")]
    pub version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reader_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<TextQuoteSelector>,
    #[serde(flatten)]
    pub anchor: SourceAnchor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextQuoteSelector {
    pub exact: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceAnchor {
    Segment {
        segment_order: i32,
        total_segments: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        segment_id: Option<String>,
    },
    TextRange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        segment_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        segment_order: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_pages: Option<u32>,
        start_offset: u32,
        end_offset: u32,
    },
    Page {
        page: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_pages: Option<u32>,
    },
    #[serde(alias = "cfi")]
    EpubCfi { cfi: String },
    #[serde(alias = "time_range")]
    Time {
        current_time: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_time: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        segment_id: Option<String>,
    },
}

impl SourceLocator {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != SOURCE_LOCATOR_VERSION {
            return Err(format!(
                "unsupported source locator version {}",
                self.version
            ));
        }
        if let Some(hash) = self.content_sha256.as_deref() {
            if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("content_sha256 must be a 64-character hexadecimal digest".to_string());
            }
        }
        if let Some(quote) = self.quote.as_ref() {
            if quote.exact.trim().is_empty() {
                return Err("quote.exact must not be empty".to_string());
            }
        }
        match &self.anchor {
            SourceAnchor::Segment {
                segment_order,
                total_segments,
                ..
            } => {
                if *segment_order < 0 || *total_segments < 1 {
                    return Err("segment locator position is invalid".to_string());
                }
            }
            SourceAnchor::TextRange {
                page,
                total_pages,
                start_offset,
                end_offset,
                ..
            } => {
                if start_offset >= end_offset {
                    return Err("text range must have a positive length".to_string());
                }
                if self.quote.is_none() {
                    return Err("text range locator requires a quote selector".to_string());
                }
                if page.is_some_and(|value| value < 1)
                    || page
                        .zip(*total_pages)
                        .is_some_and(|(page, total)| total < page)
                {
                    return Err("text range page is invalid".to_string());
                }
            }
            SourceAnchor::Page { page, total_pages } => {
                if *page < 1 || total_pages.is_some_and(|total| total < *page) {
                    return Err("page locator position is invalid".to_string());
                }
            }
            SourceAnchor::EpubCfi { cfi } => {
                if cfi.trim().is_empty() {
                    return Err("EPUB CFI must not be empty".to_string());
                }
            }
            SourceAnchor::Time {
                current_time,
                end_time,
                duration,
                ..
            } => {
                if !current_time.is_finite() || *current_time < 0.0 {
                    return Err("time locator start is invalid".to_string());
                }
                if end_time.is_some_and(|end| !end.is_finite() || end < *current_time) {
                    return Err("time locator end is invalid".to_string());
                }
                if duration.is_some_and(|value| !value.is_finite() || value < 0.0) {
                    return Err("time locator duration is invalid".to_string());
                }
            }
        }
        Ok(())
    }
}

const fn default_version() -> u8 {
    SOURCE_LOCATOR_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_legacy_reading_progress_without_version() {
        let locator: SourceLocator = serde_json::from_value(serde_json::json!({
            "kind": "segment",
            "segment_order": 2,
            "total_segments": 5,
            "segment_id": "segment-2"
        }))
        .unwrap();

        assert_eq!(locator.version, SOURCE_LOCATOR_VERSION);
        locator.validate().unwrap();
    }

    #[test]
    fn validates_versioned_text_range_contract() {
        let locator: SourceLocator = serde_json::from_value(serde_json::json!({
            "version": 1,
            "kind": "text_range",
            "segment_id": "segment-2",
            "start_offset": 4,
            "end_offset": 12,
            "material_revision": "revision-1",
            "content_sha256": "a".repeat(64),
            "quote": {
                "exact": "language",
                "prefix": "learn ",
                "suffix": " well"
            }
        }))
        .unwrap();

        locator.validate().unwrap();
        let serialized = serde_json::to_value(locator).unwrap();
        assert_eq!(serialized["kind"], "text_range");
        assert_eq!(serialized["version"], 1);
    }

    #[test]
    fn accepts_cfi_and_time_range_aliases() {
        let cfi: SourceLocator = serde_json::from_value(serde_json::json!({
            "version": 1,
            "kind": "cfi",
            "cfi": "epubcfi(/6/2!/4/1:0)"
        }))
        .unwrap();
        cfi.validate().unwrap();

        let time: SourceLocator = serde_json::from_value(serde_json::json!({
            "version": 1,
            "kind": "time_range",
            "current_time": 12.0,
            "end_time": 18.0,
            "duration": 60.0
        }))
        .unwrap();
        time.validate().unwrap();
    }

    #[test]
    fn rejects_unstable_text_range_without_quote() {
        let locator: SourceLocator = serde_json::from_value(serde_json::json!({
            "version": 1,
            "kind": "text_range",
            "segment_order": 1,
            "start_offset": 2,
            "end_offset": 8
        }))
        .unwrap();

        assert!(locator.validate().unwrap_err().contains("quote selector"));
    }
}
