//! A strict JSON reader for local command records and stdio messages.
//! Keep numbers as source text: request IDs must not lose integer precision.
use super::{quote, Result};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> Result<&Self> {
        match self {
            Self::Object(fields) => fields
                .get(key)
                .ok_or_else(|| format!("missing {key}").into()),
            _ => Err("expected a JSON object".into()),
        }
    }
    pub fn string(&self) -> Result<&str> {
        match self {
            Self::String(text) => Ok(text),
            _ => Err("expected a JSON string".into()),
        }
    }
    pub fn array(&self) -> Result<&[Self]> {
        match self {
            Self::Array(values) => Ok(values),
            _ => Err("expected a JSON array".into()),
        }
    }
    pub fn render(&self) -> String {
        match self {
            Self::Null => "null".into(),
            Self::Bool(value) => value.to_string(),
            Self::Number(text) => text.clone(),
            Self::String(text) => quote(text),
            Self::Array(values) => {
                let items = values.iter().map(Self::render).collect::<Vec<_>>();
                format!("[{}]", items.join(","))
            }
            Self::Object(fields) => {
                let items = fields
                    .iter()
                    .map(|(k, v)| format!("{}:{}", quote(k), v.render()))
                    .collect::<Vec<_>>();
                format!("{{{}}}", items.join(","))
            }
        }
    }
}

pub fn parse(text: &str) -> Result<Json> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    let value = reader.value(0)?;
    reader.space();
    if reader.at != reader.bytes.len() {
        return Err("trailing JSON input".into());
    }
    Ok(value)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn space(&mut self) {
        while matches!(self.bytes.get(self.at), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.at += 1;
        }
    }
    fn byte(&mut self, expected: u8) -> Result<()> {
        if self.bytes.get(self.at) != Some(&expected) {
            return Err(format!("expected JSON byte {expected} at {}", self.at).into());
        }
        self.at += 1;
        Ok(())
    }
    fn hex(&mut self) -> Result<u32> {
        let bytes = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or("short Unicode escape")?;
        if !bytes.iter().all(u8::is_ascii_hexdigit) {
            return Err("invalid Unicode escape".into());
        }
        self.at += 4;
        Ok(u32::from_str_radix(std::str::from_utf8(bytes)?, 16)?)
    }
    fn string(&mut self) -> Result<String> {
        self.byte(b'"')?;
        let mut out = String::new();
        loop {
            let start = self.at;
            while self
                .bytes
                .get(self.at)
                .is_some_and(|b| *b >= 32 && *b != b'"' && *b != b'\\')
            {
                self.at += 1;
            }
            out.push_str(std::str::from_utf8(&self.bytes[start..self.at])?);
            match self.bytes.get(self.at) {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => self.at += 1,
                _ => return Err("unterminated or unescaped JSON string".into()),
            }
            let escape = *self.bytes.get(self.at).ok_or("short JSON escape")?;
            self.at += 1;
            let c = match escape {
                b'"' => '"',
                b'\\' => '\\',
                b'/' => '/',
                b'b' => '\u{8}',
                b'f' => '\u{c}',
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                b'u' => {
                    let mut scalar = self.hex()?;
                    if (0xd800..=0xdbff).contains(&scalar) {
                        self.byte(b'\\')?;
                        self.byte(b'u')?;
                        let low = self.hex()?;
                        if !(0xdc00..=0xdfff).contains(&low) {
                            return Err("invalid surrogate pair".into());
                        }
                        scalar = 0x10000 + ((scalar - 0xd800) << 10) + low - 0xdc00;
                    }
                    char::from_u32(scalar).ok_or("unpaired surrogate")?
                }
                _ => return Err("invalid JSON escape".into()),
            };
            out.push(c);
        }
    }
    fn digits(&mut self) -> Result<()> {
        let start = self.at;
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        if start == self.at {
            return Err("expected JSON digits".into());
        }
        Ok(())
    }
    fn value(&mut self, depth: usize) -> Result<Json> {
        if depth > 128 {
            return Err("JSON nesting exceeds 128 levels".into());
        }
        self.space();
        let first = *self.bytes.get(self.at).ok_or("missing JSON value")?;
        match first {
            b'"' => Ok(Json::String(self.string()?)),
            b'[' | b'{' => {
                self.at += 1;
                let end = if first == b'[' { b']' } else { b'}' };
                let mut array = Vec::new();
                let mut object = BTreeMap::new();
                self.space();
                if self.bytes.get(self.at) != Some(&end) {
                    loop {
                        let key = if first == b'{' {
                            self.space();
                            let key = self.string()?;
                            self.space();
                            self.byte(b':')?;
                            Some(key)
                        } else {
                            None
                        };
                        let value = self.value(depth + 1)?;
                        if let Some(key) = key {
                            if object.insert(key, value).is_some() {
                                return Err("duplicate JSON key".into());
                            }
                        } else {
                            array.push(value);
                        }
                        self.space();
                        if self.bytes.get(self.at) != Some(&b',') {
                            break;
                        }
                        self.at += 1;
                    }
                }
                self.byte(end)?;
                Ok(if first == b'[' {
                    Json::Array(array)
                } else {
                    Json::Object(object)
                })
            }
            b'n' | b't' | b'f' => {
                let (word, value): (&[u8], _) = match first {
                    b'n' => (b"null", Json::Null),
                    b't' => (b"true", Json::Bool(true)),
                    _ => (b"false", Json::Bool(false)),
                };
                for b in word {
                    self.byte(*b)?;
                }
                Ok(value)
            }
            b'-' | b'0'..=b'9' => {
                let start = self.at;
                if first == b'-' {
                    self.at += 1;
                }
                if self.bytes.get(self.at) == Some(&b'0') {
                    self.at += 1;
                } else {
                    self.digits()?;
                }
                if self.bytes.get(self.at) == Some(&b'.') {
                    self.at += 1;
                    self.digits()?;
                }
                if self.bytes.get(self.at).is_some_and(|b| b"eE".contains(b)) {
                    self.at += 1;
                    if self.bytes.get(self.at).is_some_and(|b| b"+-".contains(b)) {
                        self.at += 1;
                    }
                    self.digits()?;
                }
                Ok(Json::Number(
                    std::str::from_utf8(&self.bytes[start..self.at])?.into(),
                ))
            }
            _ => Err("invalid JSON value".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_preserve_control_characters_and_unicode() {
        for scalar in (0..=0xffff).chain([0x10000, 0x10400, 0x10ffff]) {
            if let Some(c) = char::from_u32(scalar) {
                let text = format!("before{c}after");
                let value = Json::String(text.clone());
                assert_eq!(parse(&value.render()).unwrap().string().unwrap(), text);
            }
        }
        assert_eq!(
            parse(r#""\uD801\uDC00""#).unwrap().string().unwrap(),
            "\u{10400}"
        );
        assert_eq!(
            parse(r#""\/\b\f\n\r\t""#).unwrap().string().unwrap(),
            "/\u{8}\u{c}\n\r\t"
        );
    }

    #[test]
    fn numbers_and_nested_values_round_trip_without_host_numeric_conversion() {
        for text in [
            "0",
            "-0",
            "1e+400",
            "-1.25E-400",
            "1234567890123456789012345678901234567890",
            r#"{"null":null,"boolean":false,"nested":[true,{"number":12.34},"text"]}"#,
        ] {
            let value = parse(text).unwrap();
            assert_eq!(parse(&value.render()).unwrap(), value);
            if let Json::Number(number) = value {
                assert_eq!(number, text);
            }
        }
    }

    #[test]
    fn malformed_records_are_not_partly_accepted() {
        for text in [
            "",
            "null false",
            "01",
            "-",
            "1.",
            "1e",
            "1e+",
            "NaN",
            "[1,]",
            "[",
            "{",
            "{\"a\":1,}",
            "{\"a\":1,\"\\u0061\":2}",
            "\u{b}null",
            "\"\n\"",
            r#""\ud800""#,
            r#""\udc00""#,
            r#""\ud800\u0041""#,
            r#""\u000g""#,
            r#""\q""#,
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
        let nested = format!("{}null{}", "[".repeat(129), "]".repeat(129));
        assert!(parse(&nested).is_err());
    }
}
