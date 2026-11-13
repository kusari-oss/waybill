//! A CPE 2.3 formatted string that parsed (Constitution Principle IV, #1403).

use serde::{Deserialize, Serialize};

/// A CPE 2.3 formatted string: 13 colon-separated fields (a `\:` is an
/// escaped colon inside a field), `cpe:2.3:` first, `part` one of `a`, `o`,
/// `h`, and a vendor and product that are neither empty nor `*`.
///
/// A vendorless or productless CPE would match every product of that name or
/// vendor, which is no identity at all. A CPE 2.2 URI (`cpe:/a:...`) is a
/// different binding and is not accepted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Cpe(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a CPE 2.3 formatted string with a named vendor and product: {0:?}")]
pub struct CpeError(pub String);

impl Cpe {
    pub fn parse(value: &str) -> Result<Self, CpeError> {
        let fields = split_fields(value);
        let named = |f: &str| !f.is_empty() && f != "*";
        let ok = fields.len() == 13
            && fields[0] == "cpe"
            && fields[1] == "2.3"
            && matches!(fields[2].as_str(), "a" | "o" | "h")
            && named(&fields[3])
            && named(&fields[4]);
        if ok {
            Ok(Self(value.to_string()))
        } else {
            Err(CpeError(value.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for Cpe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Cpe {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Cpe {
    type Error = CpeError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Cpe> for String {
    fn from(value: Cpe) -> Self {
        value.0
    }
}

impl PartialEq<str> for Cpe {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for Cpe {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// Split on unescaped colons, keeping escapes in the field text.
fn split_fields(value: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut escaped = false;
    for ch in value.chars() {
        let last = fields.len() - 1;
        if escaped {
            fields[last].push(ch);
            escaped = false;
        } else if ch == '\\' {
            fields[last].push(ch);
            escaped = true;
        } else if ch == ':' {
            fields.push(String::new());
        } else {
            fields[last].push(ch);
        }
    }
    fields
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn a_formatted_string_parses() {
        let c = Cpe::parse("cpe:2.3:a:gnu:bash:5.3:9:*:*:*:*:*:*").unwrap();
        assert_eq!(c.as_str(), "cpe:2.3:a:gnu:bash:5.3:9:*:*:*:*:*:*");
        assert!(Cpe::parse("cpe:2.3:o:linux:linux_kernel:6.1:*:*:*:*:*:*:*").is_ok());
        assert!(Cpe::parse("cpe:2.3:h:vendor:board:1:*:*:*:*:*:*:*").is_ok());
    }

    /// An escaped colon is part of a field, not a separator.
    #[test]
    fn an_escaped_colon_stays_in_its_field() {
        assert!(Cpe::parse(r"cpe:2.3:a:vendor:prod:1.0\:rc1:*:*:*:*:*:*:*").is_ok());
        assert!(Cpe::parse(r"cpe:2.3:a:vendor:prod:1.0\:rc1:*:*:*:*:*:*").is_err());
    }

    #[test]
    fn anything_else_is_rejected() {
        for bad in [
            "",
            "cpe:/a:gnu:bash:5.3",
            "cpe:2.2:a:gnu:bash:5.3:*:*:*:*:*:*:*",
            "cpe:2.3:x:gnu:bash:5.3:*:*:*:*:*:*:*",
            "cpe:2.3:a:*:bash:5.3:*:*:*:*:*:*:*",
            "cpe:2.3:a::bash:5.3:*:*:*:*:*:*:*",
            "cpe:2.3:a:gnu:*:5.3:*:*:*:*:*:*:*",
            "cpe:2.3:a:gnu:bash:5.3:*:*:*:*:*:*",
            "cpe:2.3:a:gnu:bash:5.3:*:*:*:*:*:*:*:*",
        ] {
            assert!(Cpe::parse(bad).is_err(), "{bad:?}");
        }
    }

    /// Serialized as the bare string, so emitted documents do not change.
    #[test]
    fn it_serializes_as_its_string_and_validates_on_the_way_in() {
        let c = Cpe::parse("cpe:2.3:a:gnu:bash:5.3:*:*:*:*:*:*:*").unwrap();
        assert_eq!(serde_json::to_string(&c).unwrap(), "\"cpe:2.3:a:gnu:bash:5.3:*:*:*:*:*:*:*\"");
        let back: Cpe = serde_json::from_str("\"cpe:2.3:a:gnu:bash:5.3:*:*:*:*:*:*:*\"").unwrap();
        assert_eq!(back, c);
        assert!(serde_json::from_str::<Cpe>("\"cpe:/a:gnu:bash\"").is_err());
    }
}
