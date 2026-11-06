use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Pdf,
    Docx,
    Text,
    Markdown,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TextLocation {
    pub page: Option<u32>,
    pub paragraph: u32,
    pub heading: Option<String>,
    pub start_char: u32,
    pub end_char: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TextBlock {
    pub text: String,
    pub location: TextLocation,
}
