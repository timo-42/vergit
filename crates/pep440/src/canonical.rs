//! Strict validation of the canonical normal form.
//!
//! Deliberately independent of [`crate::parse`] so that round-trip tests
//! (`parse(x).to_string()` is canonical) actually check something.

use crate::parse::Scan;

/// Whether `s` is already in PEP 440 canonical form.
///
/// This is Appendix B's `is_canonical` grammar:
///
/// ```text
/// ([1-9][0-9]*!)?(0|[1-9][0-9]*)(\.(0|[1-9][0-9]*))*
/// ((a|b|rc)(0|[1-9][0-9]*))?(\.post(0|[1-9][0-9]*))?(\.dev(0|[1-9][0-9]*))?
/// ```
///
/// extended with an optional `+local` label, which the appendix omits because
/// public index servers must reject local versions.
///
/// ```
/// # use pep440::is_canonical;
/// assert!(is_canonical("1.2.4.dev5+g1a2b3c4"));
/// assert!(!is_canonical("v1.2"));   // `v` prefix
/// assert!(!is_canonical("1.2a"));   // implicit pre-release number
/// assert!(!is_canonical("1.0-1"));  // implicit post-release
/// ```
pub fn is_canonical(s: &str) -> bool {
    let mut s = Scan::new(s);

    // Epoch is written only when non-zero, hence `[1-9]` not `0`.
    let save = s.pos();
    match s.digits() {
        Some(d) if s.eat(b'!') => {
            if !no_leading_zero(d) || d == "0" {
                return false;
            }
        }
        _ => s.seek(save),
    }

    // Release: at least one component, dot-separated.
    loop {
        if !s.digits().is_some_and(no_leading_zero) {
            return false;
        }
        // A `.` only continues the release if a digit follows; otherwise it
        // belongs to `.post` or `.dev`.
        let save = s.pos();
        if !s.eat(b'.') {
            break;
        }
        if !s.peek().is_some_and(|b| b.is_ascii_digit()) {
            s.seek(save);
            break;
        }
    }

    // Pre-release: canonical spellings only, explicit number required.
    if ["a", "b", "rc"].iter().any(|a| s.eat_str(a)) && !s.digits().is_some_and(no_leading_zero) {
        return false;
    }

    for suffix in [".post", ".dev"] {
        if s.eat_str(suffix) && !s.digits().is_some_and(no_leading_zero) {
            return false;
        }
    }

    if s.eat(b'+') {
        loop {
            let Some(seg) = s.alnum() else {
                return false;
            };
            // Canonical local labels are lowercase, and numeric segments carry
            // no leading zeros.
            if seg.bytes().any(|b| b.is_ascii_uppercase()) {
                return false;
            }
            if seg.bytes().all(|b| b.is_ascii_digit()) && !no_leading_zero(seg) {
                return false;
            }
            if !s.eat(b'.') {
                break;
            }
        }
    }

    s.done()
}

fn no_leading_zero(d: &str) -> bool {
    d == "0" || !d.starts_with('0')
}
