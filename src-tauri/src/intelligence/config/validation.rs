use super::{ModelProfile, ServiceKind, configuration};
use crate::error::AppResult;
use url::{Host, Url};

pub(crate) fn validate_profile(profile: &ModelProfile) -> AppResult<()> {
    let valid_text = |value: &str, max: usize| {
        !value.trim().is_empty()
            && value.chars().count() <= max
            && !value.chars().any(char::is_control)
    };
    if !valid_text(&profile.name, 80) || !(1000..=120000).contains(&profile.timeout_ms) {
        return Err(configuration());
    }
    endpoint(profile)?;
    match profile.kind {
        ServiceKind::Llm => {
            let context = profile.max_context_tokens.ok_or_else(configuration)?;
            let output = profile.max_output_tokens.ok_or_else(configuration)?;
            if !profile
                .model
                .as_deref()
                .is_some_and(|model| valid_text(model, 200))
                || !(2048..=131072).contains(&context)
                || !(128..=16384).contains(&output)
                || output > context / 2
                || profile.dimensions.is_some()
            {
                return Err(configuration());
            }
        }
        ServiceKind::Embedding => {
            if !profile
                .model
                .as_deref()
                .is_some_and(|model| valid_text(model, 200))
                || profile.max_context_tokens.is_some()
                || profile.max_output_tokens.is_some()
                || profile
                    .dimensions
                    .is_some_and(|value| !(128..=4096).contains(&value))
            {
                return Err(configuration());
            }
        }
        ServiceKind::Tavily => {
            if profile.model.is_some()
                || profile.max_context_tokens.is_some()
                || profile.max_output_tokens.is_some()
                || profile.dimensions.is_some()
            {
                return Err(configuration());
            }
        }
    }
    Ok(())
}

/// Append the service path once, retaining a caller's base path such as /v1.
/// Query/fragment/userinfo are forbidden so credentials never enter endpoint DTOs.
pub(crate) fn endpoint(profile: &ModelProfile) -> AppResult<Url> {
    let raw = &profile.base_url;
    if raw.is_empty()
        || raw.len() > 2048
        || raw
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return Err(configuration());
    }
    let mut url = Url::parse(raw).map_err(|_| configuration())?;
    let loopback = match url.host() {
        Some(Host::Domain(name)) => name == "localhost",
        Some(Host::Ipv4(ip)) => ip.octets() == [127, 0, 0, 1],
        Some(Host::Ipv6(ip)) => ip == std::net::Ipv6Addr::LOCALHOST,
        None => false,
    };
    if url.host().is_none()
        || !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(configuration());
    }
    let suffix = match profile.kind {
        ServiceKind::Llm => "/chat/completions",
        ServiceKind::Embedding => "/embeddings",
        ServiceKind::Tavily => return Ok(url),
    };
    let path = url.path().trim_end_matches('/');
    let path = if path.ends_with(suffix) {
        path.to_string()
    } else {
        format!("{path}{suffix}")
    };
    url.set_path(&path);
    Ok(url)
}
