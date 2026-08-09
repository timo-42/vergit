//! Parse, normalize, render and compare [PEP 440] version identifiers.
//!
//! [`Version`] parses the permissive input grammar (aliases, arbitrary
//! separators, `v` prefix, implicit numbers) and its [`Display`] impl renders
//! the canonical normal form. [`Ord`] implements the PEP's ordering rules.
//!
//! ```
//! use pep440::Version;
//! let v: Version = "v1.0-RC-1".parse().unwrap();
//! assert_eq!(v.to_string(), "1.0rc1");
//! assert!("1.0.dev0".parse::<Version>().unwrap() < v);
//! ```
//!
//! This crate has no dependencies and knows nothing about git.
//!
//! # Conformance
//!
//! Checked against Python's `packaging` over a ~30k-version corpus
//! (`scripts/differential_test.py`), with one deliberate divergence: numeric
//! segments are [`u64`], not arbitrary-precision integers. A version whose
//! epoch, release, pre, post or dev number exceeds [`u64::MAX`] is rejected
//! with [`ParseError::NumberTooLarge`] rather than truncated. Keeping the
//! binary small is worth more here than versions above 18 quintillion.
//!
//! [PEP 440]: https://peps.python.org/pep-0440/

use core::cmp::Ordering;
use core::fmt;
use core::str::FromStr;

mod canonical;
mod parse;

pub use canonical::is_canonical;

/// A PEP 440 version identifier.
///
/// Values are always normalized: constructing one via [`FromStr`] applies every
/// normalization rule in the PEP, so [`Display`] output is canonical and two
/// spellings of the same version compare equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    epoch: u64,
    release: Vec<u64>,
    pre: Option<(PreKind, u64)>,
    post: Option<u64>,
    dev: Option<u64>,
    local: Vec<LocalSeg>,
}

/// The phase of a pre-release segment, in ordering order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreKind {
    /// `aN` — also spelled `alpha`.
    A,
    /// `bN` — also spelled `beta`.
    B,
    /// `rcN` — also spelled `c`, `pre` or `preview`.
    Rc,
}

impl PreKind {
    fn as_str(self) -> &'static str {
        match self {
            PreKind::A => "a",
            PreKind::B => "b",
            PreKind::Rc => "rc",
        }
    }
}

/// Which component of the release segment a bump targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// `X.0.0`
    Major = 0,
    /// `x.Y.0`
    Minor = 1,
    /// `x.y.Z`
    Patch = 2,
}

/// Why a requested next version does not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpError {
    /// A pre-release phase was requested from a version that is not already a
    /// pre-release, so which release it precedes is ambiguous.
    PhaseWithoutLevel,
    /// The requested phase sorts before the current one.
    PhaseWentBackwards {
        /// The phase the current version is in.
        from: PreKind,
        /// The phase that was asked for.
        to: PreKind,
    },
}

impl fmt::Display for BumpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BumpError::PhaseWithoutLevel => f.write_str(
                "a pre-release phase needs a release level too \
                 (which release is it a pre-release of?)",
            ),
            BumpError::PhaseWentBackwards { from, to } => write!(
                f,
                "cannot go back from phase {} to {}",
                from.as_str(),
                to.as_str()
            ),
        }
    }
}

impl std::error::Error for BumpError {}

/// One dot-separated component of a local version label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalSeg {
    /// All-digit segment; compares numerically.
    Num(u64),
    /// Segment containing letters; compares lexicographically.
    Str(String),
}

impl fmt::Display for LocalSeg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocalSeg::Num(n) => write!(f, "{n}"),
            LocalSeg::Str(s) => f.write_str(s),
        }
    }
}

/// Why a string is not a valid PEP 440 version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// Input was empty or only whitespace.
    Empty,
    /// Input contained non-ASCII bytes.
    NonAscii,
    /// The mandatory release segment was missing or malformed.
    BadRelease,
    /// A numeric segment did not fit in a `u64`.
    NumberTooLarge,
    /// A local label was empty or contained illegal characters.
    BadLocal,
    /// Input had trailing characters that are not part of a version.
    Trailing,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ParseError::Empty => "empty version string",
            ParseError::NonAscii => "version contains non-ASCII characters",
            ParseError::BadRelease => "missing or malformed release segment",
            ParseError::NumberTooLarge => "numeric segment is too large",
            ParseError::BadLocal => "malformed local version label after '+'",
            ParseError::Trailing => "unexpected trailing characters",
        })
    }
}

impl std::error::Error for ParseError {}

impl FromStr for Version {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse::parse(s)
    }
}

impl Version {
    /// The epoch segment. Implicitly `0` when absent.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The release segment, e.g. `[1, 2, 3]` for `1.2.3`. Never empty.
    pub fn release(&self) -> &[u64] {
        &self.release
    }

    /// The pre-release segment, if any.
    pub fn pre(&self) -> Option<(PreKind, u64)> {
        self.pre
    }

    /// The post-release number, if any.
    pub fn post(&self) -> Option<u64> {
        self.post
    }

    /// The developmental release number, if any.
    pub fn dev(&self) -> Option<u64> {
        self.dev
    }

    /// The local version label segments. Empty when there is no `+` part.
    pub fn local(&self) -> &[LocalSeg] {
        &self.local
    }

    /// Whether this is a pre-release, developmental release, or both.
    ///
    /// Per the PEP, post-releases of finals are *not* pre-releases.
    pub fn is_prerelease(&self) -> bool {
        self.pre.is_some() || self.dev.is_some()
    }

    /// Replace the local version label.
    pub fn set_local(&mut self, local: Vec<LocalSeg>) {
        self.local = local;
    }

    /// Set the developmental release number.
    pub fn set_dev(&mut self, dev: Option<u64>) {
        self.dev = dev;
    }

    /// Advance to the next version under development, discarding any `.postN`,
    /// `.devN` and local label.
    ///
    /// The result is the version this one is *working toward*, so that a
    /// subsequent `.devN` sorts between the two:
    ///
    /// - `1.2.3` → `1.2.4` (last release component)
    /// - `1.2` → `1.3`
    /// - `1.2.3rc1` → `1.2.3rc2` (bump the pre-release rather than skipping the
    ///   remaining candidates for `1.2.3`)
    /// - `1.2.3.post1` → `1.2.3.post2`
    ///
    /// ```
    /// # use pep440::Version;
    /// let mut v: Version = "1.2.3".parse().unwrap();
    /// v.bump_for_dev();
    /// assert_eq!(v.to_string(), "1.2.4");
    /// ```
    /// The smallest release at `level` that sorts strictly above this version,
    /// optionally as a pre-release of that release.
    ///
    /// This is the version to tag next. Post, dev and local parts are dropped;
    /// the epoch is kept.
    ///
    /// Because a pre-release is a candidate *for* its release, finalizing falls
    /// out of the same rule — the next patch-level release above `1.2.4rc1` is
    /// `1.2.4` itself:
    ///
    /// | this version | `Patch` | `Minor` | `Major` |
    /// |---|---|---|---|
    /// | `1.2.3` | `1.2.4` | `1.3.0` | `2.0.0` |
    /// | `1.2.4rc1` | `1.2.4` | `1.3.0` | `2.0.0` |
    /// | `1.3.0rc1` | `1.3.0` | `1.3.0` | `2.0.0` |
    ///
    /// With a `phase`, the result is a pre-release instead. Staying in the same
    /// phase advances its number, and a later phase restarts at 1:
    ///
    /// ```
    /// # use pep440::{Level, PreKind, Version};
    /// let v: Version = "1.2.4a1".parse().unwrap();
    /// assert_eq!(v.next(None, Some(PreKind::A)).unwrap().to_string(), "1.2.4a2");
    /// assert_eq!(v.next(None, Some(PreKind::B)).unwrap().to_string(), "1.2.4b1");
    ///
    /// let v: Version = "1.2.3".parse().unwrap();
    /// let next = v.next(Some(Level::Minor), Some(PreKind::A)).unwrap();
    /// assert_eq!(next.to_string(), "1.3.0a1");
    /// ```
    ///
    /// Errors if `phase` moves backwards, or if `phase` is given without a
    /// `level` for a version that is not already a pre-release.
    pub fn next(&self, level: Option<Level>, phase: Option<PreKind>) -> Result<Version, BumpError> {
        if level.is_none() && phase.is_some() && self.pre.is_none() {
            return Err(BumpError::PhaseWithoutLevel);
        }

        let release = match level {
            None => self.release.clone(),
            Some(level) => {
                let idx = level as usize;
                // These flags address major.minor.patch, so work in exactly
                // three components regardless of how the tag was written.
                let mut base = self.release.clone();
                base.resize(3, 0);
                base[idx + 1..].fill(0);

                // If that release is already above us we are finalizing a
                // pre-release of it, so there is nothing to increment.
                if self.with_release(&base) > *self {
                    base
                } else {
                    base[idx] += 1;
                    base
                }
            }
        };

        let pre = match phase {
            None => None,
            Some(want) => {
                let staying = cmp_release(&release, &self.release) == Ordering::Equal;
                match self.pre.filter(|_| staying) {
                    Some((from, n)) if want == from => Some((want, n + 1)),
                    Some((from, _)) if want < from => {
                        return Err(BumpError::PhaseWentBackwards { from, to: want })
                    }
                    // A new release, or a later phase of this one.
                    _ => Some((want, 1)),
                }
            }
        };

        Ok(Version {
            epoch: self.epoch,
            release,
            pre,
            post: None,
            dev: None,
            local: Vec::new(),
        })
    }

    /// This version's epoch with a different release segment and no suffixes.
    fn with_release(&self, release: &[u64]) -> Version {
        Version {
            epoch: self.epoch,
            release: release.to_vec(),
            pre: None,
            post: None,
            dev: None,
            local: Vec::new(),
        }
    }

    pub fn bump_for_dev(&mut self) {
        self.dev = None;
        self.local.clear();
        if let Some((kind, n)) = self.pre {
            self.pre = Some((kind, n.saturating_add(1)));
        } else if let Some(n) = self.post {
            self.post = Some(n.saturating_add(1));
        } else {
            let last = self
                .release
                .last_mut()
                .expect("release segment is never empty");
            *last = last.saturating_add(1);
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.epoch != 0 {
            write!(f, "{}!", self.epoch)?;
        }
        for (i, n) in self.release.iter().enumerate() {
            if i > 0 {
                f.write_str(".")?;
            }
            write!(f, "{n}")?;
        }
        if let Some((kind, n)) = self.pre {
            write!(f, "{}{}", kind.as_str(), n)?;
        }
        if let Some(n) = self.post {
            write!(f, ".post{n}")?;
        }
        if let Some(n) = self.dev {
            write!(f, ".dev{n}")?;
        }
        for (i, seg) in self.local.iter().enumerate() {
            f.write_str(if i == 0 { "+" } else { "." })?;
            write!(f, "{seg}")?;
        }
        Ok(())
    }
}

/// Sort key wrapper giving `None` an explicit position relative to `Some`.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Bound<T> {
    NegInf,
    Val(T),
    Inf,
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.epoch
            .cmp(&other.epoch)
            .then_with(|| cmp_release(&self.release, &other.release))
            .then_with(|| self.pre_key().cmp(&other.pre_key()))
            // Absent post sorts before any post: 1.0 < 1.0.post1.
            .then_with(|| bound(self.post).cmp(&bound(other.post)))
            // Absent dev sorts after any dev: 1.0.dev1 < 1.0.
            .then_with(|| {
                self.dev
                    .map_or(Bound::Inf, Bound::Val)
                    .cmp(&other.dev.map_or(Bound::Inf, Bound::Val))
            })
            .then_with(|| cmp_local(&self.local, &other.local))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Version {
    /// A version whose only suffix is `.devN` sorts before *every* pre-release
    /// of the same release; one with no pre-release at all sorts after them.
    fn pre_key(&self) -> Bound<(PreKind, u64)> {
        match self.pre {
            Some(p) => Bound::Val(p),
            None if self.post.is_none() && self.dev.is_some() => Bound::NegInf,
            None => Bound::Inf,
        }
    }
}

fn bound<T>(v: Option<T>) -> Bound<T> {
    v.map_or(Bound::NegInf, Bound::Val)
}

/// Compare release segments with trailing zeros stripped, so `1.0` == `1`.
fn cmp_release(a: &[u64], b: &[u64]) -> Ordering {
    fn trim(v: &[u64]) -> &[u64] {
        let mut end = v.len();
        while end > 0 && v[end - 1] == 0 {
            end -= 1;
        }
        &v[..end]
    }
    trim(a).cmp(trim(b))
}

fn cmp_local(a: &[LocalSeg], b: &[LocalSeg]) -> Ordering {
    // A version with no local label sorts before the same version with one.
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        (false, false) => {}
    }
    // Numeric segments always sort above lexicographic ones; otherwise compare
    // like with like. A prefix sorts before the longer label.
    let key = |s: &LocalSeg| match s {
        LocalSeg::Num(n) => (Bound::Val(*n), String::new()),
        LocalSeg::Str(s) => (Bound::NegInf, s.clone()),
    };
    a.iter().map(key).cmp(b.iter().map(key))
}

#[cfg(test)]
mod tests;
