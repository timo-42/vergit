//! The permissive input grammar, plus every normalization rule in the PEP.

use crate::{LocalSeg, ParseError, PreKind, Version};

/// Pre-release spellings, longest first so `alpha` is not read as `a`.
const PRE_ALIASES: &[(&str, PreKind)] = &[
    ("alpha", PreKind::A),
    ("beta", PreKind::B),
    ("preview", PreKind::Rc),
    ("pre", PreKind::Rc),
    ("rc", PreKind::Rc),
    ("a", PreKind::A),
    ("b", PreKind::B),
    ("c", PreKind::Rc),
];

/// Post-release spellings, longest first so `rev` is not read as `r`.
const POST_ALIASES: &[&str] = &["post", "rev", "r"];

pub(crate) fn parse(input: &str) -> Result<Version, ParseError> {
    // Surrounding whitespace is silently ignored; letters are case-insensitive
    // and normalize to lowercase.
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }
    if !trimmed.is_ascii() {
        return Err(ParseError::NonAscii);
    }
    let lowered = trimmed.to_ascii_lowercase();
    let mut s = Scan::new(&lowered);

    // An optional `v` prefix is ignored and omitted from the normal form.
    s.eat(b'v');

    let epoch = parse_epoch(&mut s)?;
    let release = parse_release(&mut s)?;
    let pre = parse_pre(&mut s)?;
    let post = parse_post(&mut s)?;
    let dev = parse_dev(&mut s)?;
    let local = parse_local(&mut s)?;

    if !s.done() {
        return Err(ParseError::Trailing);
    }
    Ok(Version {
        epoch,
        release,
        pre,
        post,
        dev,
        local,
    })
}

/// `N!`, where the digits are only an epoch if a `!` follows them.
fn parse_epoch(s: &mut Scan) -> Result<u64, ParseError> {
    let save = s.pos();
    if let Some(digits) = s.digits() {
        if s.eat(b'!') {
            return to_u64(digits);
        }
    }
    s.seek(save);
    Ok(0)
}

/// `N(.N)*` — the only mandatory segment.
fn parse_release(s: &mut Scan) -> Result<Vec<u64>, ParseError> {
    let first = s.digits().ok_or(ParseError::BadRelease)?;
    let mut out = vec![to_u64(first)?];
    loop {
        let save = s.pos();
        if !s.eat(b'.') {
            return Ok(out);
        }
        match s.digits() {
            Some(d) => out.push(to_u64(d)?),
            // Not a release component — could be `.post1` or `.dev1`.
            None => {
                s.seek(save);
                return Ok(out);
            }
        }
    }
}

/// `[-_.]?(a|b|rc|…)[-_.]?N?` — separators are dropped, the number defaults to 0.
fn parse_pre(s: &mut Scan) -> Result<Option<(PreKind, u64)>, ParseError> {
    let save = s.pos();
    s.eat_sep();
    let Some(kind) = PRE_ALIASES
        .iter()
        .find(|(alias, _)| s.eat_str(alias))
        .map(|&(_, kind)| kind)
    else {
        s.seek(save);
        return Ok(None);
    };
    s.eat_sep();
    let n = match s.digits() {
        Some(d) => to_u64(d)?,
        None => 0,
    };
    Ok(Some((kind, n)))
}

/// `-N` (the implicit form, `1.0-1` → `1.0.post1`) or `[-_.]?(post|rev|r)[-_.]?N?`.
fn parse_post(s: &mut Scan) -> Result<Option<u64>, ParseError> {
    let save = s.pos();
    if s.eat(b'-') {
        if let Some(d) = s.digits() {
            return to_u64(d).map(Some);
        }
        s.seek(save);
    }

    s.eat_sep();
    if !POST_ALIASES.iter().any(|alias| s.eat_str(alias)) {
        s.seek(save);
        return Ok(None);
    }
    s.eat_sep();
    match s.digits() {
        Some(d) => to_u64(d).map(Some),
        None => Ok(Some(0)),
    }
}

/// `[-_.]?dev[-_.]?N?`.
fn parse_dev(s: &mut Scan) -> Result<Option<u64>, ParseError> {
    let save = s.pos();
    s.eat_sep();
    if !s.eat_str("dev") {
        s.seek(save);
        return Ok(None);
    }
    s.eat_sep();
    match s.digits() {
        Some(d) => to_u64(d).map(Some),
        None => Ok(Some(0)),
    }
}

/// `+label`, where `.`, `-` and `_` separators all normalize to `.`.
fn parse_local(s: &mut Scan) -> Result<Vec<LocalSeg>, ParseError> {
    if !s.eat(b'+') {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    loop {
        let seg = s.alnum().ok_or(ParseError::BadLocal)?;
        // A segment of only digits is an integer for comparison purposes,
        // which also strips its leading zeros.
        out.push(match seg.bytes().all(|b| b.is_ascii_digit()) {
            true => LocalSeg::Num(to_u64(seg)?),
            false => LocalSeg::Str(seg.to_owned()),
        });
        if !s.eat_sep() {
            return Ok(out);
        }
    }
}

/// Parses via `int()` semantics, so leading zeros are dropped.
fn to_u64(digits: &str) -> Result<u64, ParseError> {
    digits.parse().map_err(|_| ParseError::NumberTooLarge)
}

/// A byte cursor over ASCII input.
pub(crate) struct Scan<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Scan<'a> {
    pub(crate) fn new(s: &'a str) -> Self {
        Scan {
            s: s.as_bytes(),
            i: 0,
        }
    }

    pub(crate) fn pos(&self) -> usize {
        self.i
    }

    pub(crate) fn seek(&mut self, i: usize) {
        self.i = i;
    }

    pub(crate) fn done(&self) -> bool {
        self.i >= self.s.len()
    }

    pub(crate) fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    pub(crate) fn eat(&mut self, b: u8) -> bool {
        let hit = self.peek() == Some(b);
        self.i += usize::from(hit);
        hit
    }

    /// Any of the interchangeable `.`, `-`, `_` separators.
    pub(crate) fn eat_sep(&mut self) -> bool {
        matches!(self.peek(), Some(b'.' | b'-' | b'_')) && {
            self.i += 1;
            true
        }
    }

    pub(crate) fn eat_str(&mut self, t: &str) -> bool {
        let hit = self.s[self.i..].starts_with(t.as_bytes());
        self.i += if hit { t.len() } else { 0 };
        hit
    }

    pub(crate) fn digits(&mut self) -> Option<&'a str> {
        self.take_while(u8::is_ascii_digit)
    }

    pub(crate) fn alnum(&mut self) -> Option<&'a str> {
        self.take_while(u8::is_ascii_alphanumeric)
    }

    fn take_while(&mut self, f: impl Fn(&u8) -> bool) -> Option<&'a str> {
        let start = self.i;
        while self.peek().is_some_and(|b| f(&b)) {
            self.i += 1;
        }
        if self.i == start {
            return None;
        }
        // Safe: input was verified ASCII, so every index is a char boundary.
        Some(core::str::from_utf8(&self.s[start..self.i]).expect("ascii"))
    }
}
