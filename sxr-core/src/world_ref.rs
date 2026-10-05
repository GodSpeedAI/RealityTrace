//! Semantic-world identity gate (CEP-0008 `world_ref`).
//!
//! A `world_ref` names exactly one resolved semantic-world revision:
//! `world:<name>@sha256:<64 lowercase hex>`. DomainForge defines it and
//! SEA-Forge recomputes its digest; this crate checks SYNTAX and EQUALITY
//! against the deployment's pinned world and never claims the digest was
//! verified here. An alias (`world:<name>`) or label is not an identity.

/// Maximum length of the world name (same bound as the cep `world_ref` schema).
const NAME_MAX: usize = 64;

/// True when `value` is a well-formed pinned `world_ref`.
pub fn is_world_ref(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("world:") else {
        return false;
    };
    let Some((name, digest)) = rest.split_once("@sha256:") else {
        return false;
    };
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && valid_name(name)
}

/// `[a-z][a-z0-9]*([._-][a-z0-9]+)*`, at most 64 characters.
fn valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > NAME_MAX {
        return false;
    }
    let mut chars = name.bytes();
    if !chars.next().is_some_and(|b| b.is_ascii_lowercase()) {
        return false;
    }
    let mut prev_sep = false;
    for b in chars {
        let alnum = b.is_ascii_digit() || b.is_ascii_lowercase();
        let sep = matches!(b, b'.' | b'_' | b'-');
        if !(alnum || sep) || (sep && prev_sep) {
            return false;
        }
        prev_sep = sep;
    }
    !prev_sep
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn accepts_pinned_refs_only() {
        assert!(is_world_ref(&format!("world:demo@sha256:{D}")));
        assert!(is_world_ref(&format!("world:a.b-c_d1@sha256:{D}")));
        for bad in [
            "world:demo".to_string(),
            format!("demo@sha256:{D}"),
            format!("world:Demo@sha256:{D}"),
            format!("world:demo@sha256:{}", &D[1..]),
            format!("world:demo@sha256:{}", "A".repeat(64)),
            format!("world:de..mo@sha256:{D}"),
            format!("world:demo-@sha256:{D}"),
            format!("world:@sha256:{D}"),
            format!("world:{}@sha256:{D}", "a".repeat(65)),
            String::new(),
        ] {
            assert!(!is_world_ref(&bad), "{bad}");
        }
    }
}
