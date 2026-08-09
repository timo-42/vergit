use super::*;

fn v(s: &str) -> Version {
    s.parse()
        .unwrap_or_else(|e| panic!("{s:?} should parse: {e}"))
}

/// The complete ordering example from the PEP, in ascending order.
const ORDERED: &[&str] = &[
    "1.dev0",
    "1.0.dev456",
    "1.0a1",
    "1.0a2.dev456",
    "1.0a12.dev456",
    "1.0a12",
    "1.0b1.dev456",
    "1.0b2",
    "1.0b2.post345.dev456",
    "1.0b2.post345",
    "1.0rc1.dev456",
    "1.0rc1",
    "1.0",
    "1.0+abc.5",
    "1.0+abc.7",
    "1.0+5",
    "1.0.post456.dev34",
    "1.0.post456",
    "1.0.15",
    "1.1.dev1",
];

#[test]
fn pep_ordering_example_is_strictly_ascending() {
    for pair in ORDERED.windows(2) {
        let (a, b) = (v(pair[0]), v(pair[1]));
        assert!(a < b, "expected {} < {}", pair[0], pair[1]);
    }
}

#[test]
fn sorting_the_shuffled_example_restores_the_pep_order() {
    // Reversed rather than randomized so the test is deterministic.
    let mut versions: Vec<Version> = ORDERED.iter().rev().map(|s| v(s)).collect();
    versions.sort();
    let got: Vec<String> = versions.iter().map(|x| x.to_string()).collect();
    assert_eq!(got, ORDERED);
}

/// `(input, canonical form)` — every normalization rule in the PEP.
const NORMALIZE: &[(&str, &str)] = &[
    // Case, whitespace and the `v` prefix.
    ("  1.0  ", "1.0"),
    ("v1.0", "1.0"),
    ("V1.0", "1.0"),
    ("1.0RC1", "1.0rc1"),
    // Integer normalization strips leading zeros.
    ("01.0", "1.0"),
    ("1.0a09000", "1.0a9000"),
    ("00!1.0", "1.0"),
    ("1.0+00", "1.0+0"),
    // Epoch.
    ("1!1.0", "1!1.0"),
    ("0!1.0", "1.0"),
    // Pre-release spellings and separators.
    ("1.0alpha1", "1.0a1"),
    ("1.0beta1", "1.0b1"),
    ("1.0c1", "1.0rc1"),
    ("1.0pre1", "1.0rc1"),
    ("1.0preview1", "1.0rc1"),
    ("1.0-rc-1", "1.0rc1"),
    ("1.0_rc_1", "1.0rc1"),
    ("1.0.rc.1", "1.0rc1"),
    // Implicit pre-release number.
    ("1.2a", "1.2a0"),
    ("1.2-alpha", "1.2a0"),
    // Post-release spellings, separators and implicit numbers.
    ("1.0.post1", "1.0.post1"),
    ("1.0-post1", "1.0.post1"),
    ("1.0_post_1", "1.0.post1"),
    ("1.0rev1", "1.0.post1"),
    ("1.0r1", "1.0.post1"),
    ("1.2.post", "1.2.post0"),
    // The implicit post-release form: a bare `-N`.
    ("1.0-1", "1.0.post1"),
    // Dev releases.
    ("1.2.dev", "1.2.dev0"),
    ("1.2-dev-3", "1.2.dev3"),
    ("1.2_DEV_3", "1.2.dev3"),
    // Local labels: all separators become `.`.
    ("1.0+ubuntu-1", "1.0+ubuntu.1"),
    ("1.0+UBUNTU_1", "1.0+ubuntu.1"),
    ("1.0+abc.1.2", "1.0+abc.1.2"),
    // Everything at once.
    (
        "  V1!2.0.0-ALPHA-3_REV_4.DEV_5+Foo-01  ",
        "1!2.0.0a3.post4.dev5+foo.1",
    ),
];

#[test]
fn normalization_matches_the_pep() {
    for &(input, want) in NORMALIZE {
        assert_eq!(v(input).to_string(), want, "normalizing {input:?}");
    }
}

#[test]
fn display_output_is_always_canonical() {
    for &(input, _) in NORMALIZE {
        let rendered = v(input).to_string();
        assert!(
            is_canonical(&rendered),
            "{input:?} rendered as non-canonical {rendered:?}"
        );
    }
    for &input in ORDERED {
        assert!(is_canonical(&v(input).to_string()));
    }
}

#[test]
fn reparsing_the_canonical_form_is_a_fixed_point() {
    for &(input, _) in NORMALIZE {
        let once = v(input);
        assert_eq!(v(&once.to_string()), once, "for {input:?}");
    }
}

#[test]
fn is_canonical_rejects_non_normal_forms() {
    for s in [
        "v1.0",
        "1.0RC1",
        "01.0",
        "1.2a",
        "1.0-1",
        "1.0alpha1",
        "0!1.0",
        "1.0.post",
        " 1.0",
        "1.0+Foo",
        "1.0+01",
        "1.0.dev",
        "1.0-dev1",
    ] {
        assert!(!is_canonical(s), "{s:?} is not canonical");
    }
    for s in [
        "1",
        "1.0",
        "1!1.0",
        "1.0a1",
        "1.0.post1.dev2",
        "1.2.4.dev5+g1a2b3c4",
    ] {
        assert!(is_canonical(s), "{s:?} is canonical");
    }
}

#[test]
fn invalid_input_is_rejected() {
    use ParseError::*;
    for (input, want) in [
        ("", Empty),
        ("   ", Empty),
        ("1.0+é", NonAscii),
        ("abc", BadRelease),
        ("a1.0", BadRelease),
        ("+1.0", BadRelease),
        ("1.0+", BadLocal),
        ("1.0+ubuntu-", BadLocal),
        ("1.0.", Trailing),
        ("1.2.3four", Trailing),
        ("1.0++1", BadLocal),
        ("1.0 dev1", Trailing),
        ("99999999999999999999999.0", NumberTooLarge),
    ] {
        assert_eq!(input.parse::<Version>(), Err(want), "for {input:?}");
    }
}

#[test]
fn equal_versions_ignore_trailing_release_zeros() {
    assert_eq!(v("1"), v("1"));
    assert_eq!(v("1").cmp(&v("1.0")), Ordering::Equal);
    assert_eq!(v("1.0").cmp(&v("1.0.0.0")), Ordering::Equal);
    assert!(v("1.0") < v("1.0.1"));
}

#[test]
fn local_segments_order_numeric_above_lexicographic() {
    assert!(v("1.0+abc") < v("1.0+1"));
    assert!(v("1.0+abc") < v("1.0+abd"));
    // A prefix sorts before the longer label.
    assert!(v("1.0+abc") < v("1.0+abc.1"));
    // Case is normalized away, so these are equal.
    assert_eq!(v("1.0+ABC"), v("1.0+abc"));
}

#[test]
fn bump_for_dev_targets_the_next_version() {
    for (from, want) in [
        ("1.2.3", "1.2.4"),
        ("1.2", "1.3"),
        ("1", "2"),
        // Bump the candidate rather than skipping the rest of 1.2.3.
        ("1.2.3rc1", "1.2.3rc2"),
        ("1.2.3a0", "1.2.3a1"),
        ("1.2.3.post1", "1.2.3.post2"),
        // Existing dev and local parts are discarded.
        ("1.2.3.dev9+gdeadbeef", "1.2.4"),
        ("1!1.2.3", "1!1.2.4"),
    ] {
        let mut got = v(from);
        got.bump_for_dev();
        assert_eq!(got.to_string(), want, "bumping {from:?}");
    }
}

#[test]
fn a_dev_version_sorts_between_its_tag_and_its_target() {
    let tag = v("1.2.3");
    let mut next = tag.clone();
    next.bump_for_dev();
    let dev: Version = format!("{next}.dev5+g1a2b3c4").parse().unwrap();
    assert!(tag < dev, "{tag} < {dev}");
    assert!(dev < next, "{dev} < {next}");
}

/// `(current, Level, expected)` — "the smallest release at this level that
/// sorts above the current version".
const NEXT_RELEASE: &[(&str, Level, &str)] = &[
    ("1.2.3", Level::Patch, "1.2.4"),
    ("1.2.3", Level::Minor, "1.3.0"),
    ("1.2.3", Level::Major, "2.0.0"),
    // A pre-release is a candidate for its release, so the next release at
    // that level is the release itself. This is how you finalize.
    ("1.2.4rc1", Level::Patch, "1.2.4"),
    ("1.2.4rc1", Level::Minor, "1.3.0"),
    ("1.2.4rc1", Level::Major, "2.0.0"),
    ("1.3.0rc1", Level::Patch, "1.3.0"),
    ("1.3.0rc1", Level::Minor, "1.3.0"),
    ("1.3.0rc1", Level::Major, "2.0.0"),
    ("2.0.0rc1", Level::Patch, "2.0.0"),
    ("2.0.0rc1", Level::Minor, "2.0.0"),
    ("2.0.0rc1", Level::Major, "2.0.0"),
    // Dev releases finalize the same way; post-releases do not, since they
    // already sort above their release.
    ("1.2.4.dev5", Level::Patch, "1.2.4"),
    ("1.2.4.post1", Level::Patch, "1.2.5"),
    // Short and long release segments normalize to major.minor.patch.
    ("1.2", Level::Patch, "1.2.1"),
    ("1", Level::Minor, "1.1.0"),
    ("0.0.0", Level::Patch, "0.0.1"),
    ("1.2.3.4", Level::Patch, "1.2.4"),
    // The epoch is carried through.
    ("2!1.2.3", Level::Minor, "2!1.3.0"),
];

#[test]
fn next_release_matches_the_table() {
    for &(from, level, want) in NEXT_RELEASE {
        let got = v(from).next(Some(level), None).unwrap();
        assert_eq!(got.to_string(), want, "next {level:?} after {from:?}");
    }
}

#[test]
fn next_is_always_strictly_greater() {
    let levels = [Level::Major, Level::Minor, Level::Patch];
    let phases = [None, Some(PreKind::A), Some(PreKind::B), Some(PreKind::Rc)];
    for &input in ORDERED
        .iter()
        .chain(["1.2.4rc1", "1.2.4.post1", "2!1.0"].iter())
    {
        let current = v(input);
        for level in levels {
            for phase in phases {
                // Rejected combinations are backwards phase moves, which is
                // exactly the invariant being enforced.
                if let Ok(next) = current.next(Some(level), phase) {
                    assert!(
                        next > current,
                        "{input}: next({level:?}, {phase:?}) = {next}"
                    );
                }
            }
        }
    }
}

#[test]
fn phases_advance_and_restart() {
    for (from, phase, want) in [
        // Same phase advances its number.
        ("1.2.4a1", PreKind::A, "1.2.4a2"),
        ("1.2.4b7", PreKind::B, "1.2.4b8"),
        // A later phase restarts at 1.
        ("1.2.4a3", PreKind::B, "1.2.4b1"),
        ("1.2.4a3", PreKind::Rc, "1.2.4rc1"),
        ("1.2.4b2", PreKind::Rc, "1.2.4rc1"),
    ] {
        let got = v(from).next(None, Some(phase)).unwrap();
        assert_eq!(got.to_string(), want, "{from:?} -> {phase:?}");
    }
}

#[test]
fn a_level_and_a_phase_combine() {
    for (from, level, phase, want) in [
        ("1.2.3", Level::Minor, PreKind::A, "1.3.0a1"),
        ("1.2.3", Level::Major, PreKind::Rc, "2.0.0rc1"),
        ("1.2.3", Level::Patch, PreKind::A, "1.2.4a1"),
        // Level lands on the release the candidate is already for, so the
        // phase advances rather than restarting.
        ("1.2.4rc1", Level::Patch, PreKind::Rc, "1.2.4rc2"),
        // A different release restarts the phase, even going "backwards".
        ("1.2.4rc1", Level::Minor, PreKind::A, "1.3.0a1"),
    ] {
        let got = v(from).next(Some(level), Some(phase)).unwrap();
        assert_eq!(got.to_string(), want, "{from:?} {level:?} {phase:?}");
    }
}

#[test]
fn phases_cannot_go_backwards_within_a_release() {
    assert_eq!(
        v("1.2.4b2").next(None, Some(PreKind::A)),
        Err(BumpError::PhaseWentBackwards {
            from: PreKind::B,
            to: PreKind::A
        })
    );
    assert_eq!(
        v("1.2.4rc1").next(Some(Level::Patch), Some(PreKind::B)),
        Err(BumpError::PhaseWentBackwards {
            from: PreKind::Rc,
            to: PreKind::B
        })
    );
}

#[test]
fn a_phase_alone_needs_an_existing_prerelease() {
    // 1.2.3 is final: an alpha of which release?
    assert_eq!(
        v("1.2.3").next(None, Some(PreKind::A)),
        Err(BumpError::PhaseWithoutLevel)
    );
    assert_eq!(
        v("1.2.3.post1").next(None, Some(PreKind::A)),
        Err(BumpError::PhaseWithoutLevel)
    );
    // But it is unambiguous when one is already in flight.
    assert!(v("1.2.3a1").next(None, Some(PreKind::B)).is_ok());
}

#[test]
fn next_drops_post_dev_and_local() {
    let got = v("1.2.3.post4.dev5+gabc")
        .next(Some(Level::Patch), None)
        .unwrap();
    assert_eq!(got.to_string(), "1.2.4");
}

#[test]
fn prerelease_classification_follows_the_pep() {
    assert!(v("1.0a1").is_prerelease());
    assert!(v("1.0.dev1").is_prerelease());
    assert!(v("1.0a1.dev1").is_prerelease());
    assert!(!v("1.0").is_prerelease());
    assert!(!v("1.0.post1").is_prerelease());
}
