use super::{Blocks, Code};
use crate::knowledge::MAX_OUTPUT_BYTES;
use quick_xml::{NsReader, events::Event, name::ResolveResult};
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Seek},
};

const WORD: &[u8] = b"http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const STRICT: &[u8] = b"http://purl.oclc.org/ooxml/wordprocessingml/main";
fn word(namespace: ResolveResult<'_>) -> Result<bool, Code> {
    match namespace {
        ResolveResult::Bound(ns) => Ok(ns.as_ref() == WORD || ns.as_ref() == STRICT),
        ResolveResult::Unbound => Ok(false),
        ResolveResult::Unknown(_) => Err(Code::ParseFailed),
    }
}
fn entry<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
    remaining: &mut usize,
) -> Result<Option<Vec<u8>>, Code> {
    let mut file = match zip.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(_) => return Err(Code::ParseFailed),
    };
    if file.encrypted() {
        return Err(Code::UnsupportedDocument);
    }
    if file.size() > *remaining as u64 {
        return Err(Code::ResourceLimit);
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(*remaining as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Code::ParseFailed)?;
    if bytes.len() > *remaining {
        return Err(Code::ResourceLimit);
    }
    *remaining -= bytes.len();
    Ok(Some(bytes))
}

pub(super) fn parse(file: impl Read + Seek, blocks: &mut Blocks) -> Result<(), Code> {
    let mut zip = zip::ZipArchive::new(file).map_err(|_| Code::ParseFailed)?;
    let mut remaining = MAX_OUTPUT_BYTES;
    let styles = entry(&mut zip, "word/styles.xml", &mut remaining)?;
    let mut headings = HashSet::new();
    if let Some(styles) = styles {
        walk(&styles, true, &mut headings, &mut Blocks::default())?;
    }
    let document =
        entry(&mut zip, "word/document.xml", &mut remaining)?.ok_or(Code::ParseFailed)?;
    walk(&document, false, &mut headings, blocks)
}

fn walk(
    bytes: &[u8],
    styles: bool,
    headings: &mut HashSet<String>,
    blocks: &mut Blocks,
) -> Result<(), Code> {
    // DOCX has a deliberate UTF-8-only policy, independent of feature unification.
    let xml = std::str::from_utf8(bytes).map_err(|_| Code::UnsupportedDocument)?;
    let xml = xml.strip_prefix('\u{feff}').unwrap_or(xml);
    let mut reader = NsReader::from_str(xml);
    reader.config_mut().expand_empty_elements = true;
    reader.config_mut().check_comments = true;
    let mut buffer = Vec::new();
    let mut stack: Vec<(bool, Vec<u8>)> = Vec::new();
    let mut roots = 0;
    let mut declared = false;
    let mut paragraphs = 0usize;
    let mut supported_root = false;
    let mut current_style = None;
    let mut heading = None;
    let mut paragraph = None::<String>;
    let mut paragraph_heading = false;
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| Code::ParseFailed)?;
        match event {
            Event::Start(element) => {
                let qualified = element.name();
                let (namespace, local) = reader.resolve_element(qualified);
                let is_word = word(namespace)?;
                let name = local.as_ref().to_vec();
                let mut attrs = HashMap::new();
                for attribute in element.attributes() {
                    let attr = attribute.map_err(|_| Code::ParseFailed)?;
                    let value = attr
                        .decode_and_unescape_value_with(
                            reader.decoder(),
                            quick_xml::escape::resolve_xml_entity,
                        )
                        .map_err(|_| Code::ParseFailed)?;
                    let (namespace, local) = reader.resolve_attribute(attr.key);
                    if word(namespace)? {
                        attrs.insert(local.as_ref().to_vec(), value.into_owned());
                    }
                }
                if stack.len() >= 64 {
                    return Err(Code::ResourceLimit);
                }
                if stack.is_empty() {
                    roots += 1;
                    if roots != 1 {
                        return Err(Code::ParseFailed);
                    }
                    supported_root = is_word
                        && name
                            == if styles {
                                b"styles".as_slice()
                            } else {
                                b"document".as_slice()
                            };
                }
                if is_word {
                    if styles {
                        match name.as_slice() {
                            b"style" => current_style = attrs.get(b"styleId".as_slice()).cloned(),
                            b"outlineLvl"
                                if attrs
                                    .get(b"val".as_slice())
                                    .is_some_and(|v| v.parse::<u8>().is_ok_and(|v| v < 9)) =>
                            {
                                if let Some(id) = &current_style {
                                    headings.insert(id.clone());
                                }
                            }
                            _ => {}
                        }
                    } else {
                        match name.as_slice() {
                            b"p" => {
                                paragraphs += 1;
                                if paragraphs > crate::knowledge::MAX_PARAGRAPHS {
                                    return Err(Code::ResourceLimit);
                                }
                                if paragraph.is_some() {
                                    return Err(Code::ParseFailed);
                                }
                                paragraph = Some(String::new());
                                paragraph_heading = false;
                            }
                            b"pStyle" => {
                                paragraph_heading = attrs
                                    .get(b"val".as_slice())
                                    .is_some_and(|v| headings.contains(v))
                            }
                            b"outlineLvl" => {
                                paragraph_heading = attrs
                                    .get(b"val".as_slice())
                                    .is_some_and(|v| v.parse::<u8>().is_ok_and(|v| v < 9))
                            }
                            b"tab" => {
                                if let Some(text) = &mut paragraph {
                                    text.push('\t');
                                }
                            }
                            b"br" | b"cr" => {
                                if let Some(text) = &mut paragraph {
                                    text.push('\n');
                                }
                            }
                            _ => {}
                        }
                    }
                }
                stack.push((is_word, name));
            }
            Event::End(_) => {
                let (is_word, name) = stack.pop().ok_or(Code::ParseFailed)?;
                if is_word && name == b"style" {
                    current_style = None;
                }
                if !styles && is_word && name == b"p" {
                    let text = paragraph.take().ok_or(Code::ParseFailed)?;
                    if paragraph_heading {
                        heading = Some(super::normalize(&text)?);
                    }
                    blocks.push(&text, None, heading.as_deref())?;
                }
            }
            Event::Text(text) => {
                let value = text.xml_content().map_err(|_| Code::ParseFailed)?;
                if stack.is_empty() && !value.trim().is_empty() {
                    return Err(Code::ParseFailed);
                }
                if stack.last().is_some_and(|(w, n)| *w && n == b"t") {
                    if let Some(text) = &mut paragraph {
                        text.push_str(&value);
                    }
                }
            }
            Event::CData(text) => {
                if stack.is_empty() {
                    return Err(Code::ParseFailed);
                }
                if stack.last().is_some_and(|(w, n)| *w && n == b"t") {
                    if let Some(value) = &mut paragraph {
                        value.push_str(&text.decode().map_err(|_| Code::ParseFailed)?);
                    }
                }
            }
            Event::GeneralRef(reference) => {
                let value = if let Some(c) = reference
                    .resolve_char_ref()
                    .map_err(|_| Code::ParseFailed)?
                {
                    c.to_string()
                } else {
                    quick_xml::escape::resolve_xml_entity(
                        &reference.decode().map_err(|_| Code::ParseFailed)?,
                    )
                    .ok_or(Code::ParseFailed)?
                    .to_owned()
                };
                if stack.is_empty() {
                    return Err(Code::ParseFailed);
                }
                if stack.last().is_some_and(|(w, n)| *w && n == b"t") {
                    if let Some(text) = &mut paragraph {
                        text.push_str(&value);
                    }
                }
            }
            Event::Decl(decl) => {
                if roots != 0 || declared {
                    return Err(Code::ParseFailed);
                }
                declared = true;
                decl.version().map_err(|_| Code::ParseFailed)?;
                if let Some(encoding) = decl.encoding() {
                    if !encoding
                        .map_err(|_| Code::ParseFailed)?
                        .eq_ignore_ascii_case(b"utf-8")
                    {
                        return Err(Code::UnsupportedDocument);
                    }
                }
            }
            Event::DocType(_) => return Err(Code::ParseFailed),
            Event::Eof => break,
            Event::PI(_) | Event::Comment(_) => {}
            Event::Empty(_) => unreachable!("expanded empty elements"),
        }
        buffer.clear();
    }
    if !stack.is_empty() || roots != 1 {
        return Err(Code::ParseFailed);
    }
    if !supported_root {
        return Err(Code::UnsupportedDocument);
    }
    Ok(())
}
