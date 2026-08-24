//! `vergit` — generate a PEP 440 version string from git state.

mod args;
mod git;

use std::process::ExitCode;

use args::{Args, OutputFormat, Parsed};
use pep440::{LocalSeg, Version};

/// Used when the repository has no tag to start from.
const UNTAGGED_BASE: &str = "0.0.0";

/// The version embedded by the release workflow, or Cargo's package version for
/// local and CI builds.
const BUILD_VERSION: &str = match option_env!("VERGIT_BUILD_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

/// The version this commit is measured from.
enum Base {
    /// `HEAD` itself carries a version tag; reproduce it verbatim.
    OnTag(Version),
    /// `HEAD` is descended from a version tag, by `distance` commits.
    AfterTag { version: Version, distance: u64 },
    /// No usable version tag in this commit's history; `count` is every commit.
    Untagged { count: u64 },
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("vergit: {msg}");
            // 2 for usage and environment errors, so `--check` keeps 1 to
            // itself and callers can tell the two apart.
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let args = match args::parse(std::env::args().skip(1))? {
        Parsed::Help => {
            print!("{}", args::HELP);
            return Ok(ExitCode::SUCCESS);
        }
        Parsed::ToolVersion => {
            println!("vergit {BUILD_VERSION}");
            return Ok(ExitCode::SUCCESS);
        }
        Parsed::Run(args) => args,
    };

    if let (Some(input), true) = (&args.from, args.check) {
        return Ok(match pep440::is_canonical(input) {
            true => ExitCode::SUCCESS,
            false => ExitCode::FAILURE,
        });
    }

    let is_next = args.is_next();
    let version = match is_next {
        true => next_version(&args)?,
        false => describe(&args)?,
    };
    let mut line = render(&version, args.format, args.separator)?;
    if is_next && args.with_prefix {
        line.insert_str(0, &args.tag_prefix);
        if args.format == OutputFormat::Docker {
            validate_docker_tag(&line)?;
        }
    }
    println!("{line}");
    Ok(ExitCode::SUCCESS)
}

/// The highest version among `tags`, with its original tag name.
///
/// Names that are not PEP 440 versions are ignored, so a `vlatest` or `vstable`
/// alongside real tags is invisible rather than fatal. A commit can carry
/// several tags, and picking the maximum makes the answer independent of how
/// each was created.
fn best<'a>(tags: &'a [String], prefix: &str) -> Option<(&'a str, Version)> {
    tags.iter()
        .filter_map(|tag| {
            let version: Version = tag.strip_prefix(prefix).unwrap_or(tag).parse().ok()?;
            Some((tag.as_str(), version))
        })
        .max_by(|a, b| a.1.cmp(&b.1))
}

/// Finds the version this commit is measured from.
///
/// The base is the **highest** version tag reachable from `HEAD`, not the
/// nearest one. `git describe` answers "nearest", which goes wrong as soon as
/// history merges: merging a `v1.0.1` hotfix branch back into a main that has
/// released `v2.0.0` leaves the hotfix tag nearest, and measuring from it would
/// stamp the build `1.0.2.devN` — below a release it already contains.
/// Measuring from the highest reachable tag keeps every developmental build
/// above every release it is built on top of.
fn resolve_base(args: &Args) -> Result<Base, String> {
    let dir = args.dir.as_deref();

    // A tag on HEAD itself wins outright, so a release checkout reproduces
    // exactly what was tagged.
    let at_head = git::tags_at(dir, "HEAD", &args.tag_prefix)?;
    if let Some((_, version)) = best(&at_head, &args.tag_prefix) {
        return Ok(Base::OnTag(version));
    }

    let reachable = git::tags_merged(dir, &args.tag_prefix)?;
    if let Some((tag, version)) = best(&reachable, &args.tag_prefix) {
        return Ok(Base::AfterTag {
            distance: git::count_since(dir, tag)?,
            version,
        });
    }

    git::ensure_not_shallow(dir)?;
    Ok(Base::Untagged {
        count: git::count_all(dir)?,
    })
}

/// Default mode: what version *is* this commit?
///
/// A clean checkout of a tag reproduces that tag exactly. Anything else is a
/// developmental release of the next version, so it sorts after the tag it came
/// from and before the release it is heading for.
fn describe(args: &Args) -> Result<Version, String> {
    if args.from.is_some() {
        // `--from` supplies the version outright; there is nothing to derive.
        let mut version = parse_from(args)?;
        if args.no_local {
            version.set_local(Vec::new());
        }
        return Ok(version);
    }

    let head = git::head(args.dir.as_deref())?;
    let (mut version, dev) = match resolve_base(args)? {
        Base::OnTag(version) => (version, None),
        // Commits past the tag are progress toward the next version.
        Base::AfterTag {
            mut version,
            distance,
        } => {
            version.bump_for_dev();
            (version, Some(distance))
        }
        // With no tag there is nothing to bump from, so stay at 0.0.0 — every
        // eventual release then sorts above these builds.
        Base::Untagged { count } => (untagged_base(), Some(count)),
    };
    version.set_dev(dev);
    let (hash, dirty) = (head.hash, head.dirty);

    if !args.no_local {
        // Exactly on a clean tag, the tag is reproduced verbatim — no local
        // label, so the output is byte-identical to what was tagged.
        let mut local = Vec::new();
        if dev.is_some() || dirty {
            local.push(LocalSeg::Str(hash));
        }
        if dirty {
            local.push(LocalSeg::Str("dirty".to_owned()));
        }
        version.set_local(local);
    }
    Ok(version)
}

/// `--next-*` mode: what version should be tagged next?
///
/// The base is the highest version tag reachable from `HEAD` — not the nearest
/// one, and not the derived developmental version. Commits since a tag do not
/// change which version comes after it, and bumping from the maximum means the
/// result cannot collide with a tag that is already reachable.
fn next_version(args: &Args) -> Result<Version, String> {
    let current = match &args.from {
        Some(_) => parse_from(args)?,
        None => {
            let tags = git::tags_merged(args.dir.as_deref(), &args.tag_prefix)?;
            match best(&tags, &args.tag_prefix) {
                Some((_, version)) => version,
                // No usable tag: start the series from scratch.
                None => untagged_base(),
            }
        }
    };

    current
        .next(args.level, args.phase)
        .map_err(|e| format!("no next version after {current}: {e}"))
}

/// Render the PEP 440 value for its destination without changing how it was
/// derived or how it compares. Docker tags admit dots, dashes and underscores,
/// but not PEP 440's `+` local-version boundary or `!` epoch marker.
fn render(version: &Version, format: OutputFormat, separator: char) -> Result<String, String> {
    match format {
        OutputFormat::Pep440 => Ok(version.to_string()),
        OutputFormat::Docker => {
            let canonical = version.to_string();
            let mut tag = match canonical.split_once('!') {
                Some((epoch, rest)) => format!("epoch{epoch}{separator}{rest}"),
                None => canonical,
            };
            if let Some(local) = tag.find('+') {
                tag.replace_range(local..=local, &separator.to_string());
            }
            validate_docker_tag(&tag)?;
            Ok(tag)
        }
    }
}

fn validate_docker_tag(tag: &str) -> Result<(), String> {
    if tag.len() > 128 {
        return Err(format!(
            "Docker tag is {} bytes; the maximum is 128",
            tag.len()
        ));
    }

    let mut chars = tag.chars();
    let valid_first = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
    let valid_rest = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    if !valid_first || !valid_rest {
        return Err(format!(
            "{tag:?} is not a valid Docker tag (allowed: ASCII letters, digits, `_`, `.` and `-`; the first character cannot be `.` or `-`)"
        ));
    }
    Ok(())
}

fn parse_from(args: &Args) -> Result<Version, String> {
    let input = args.from.as_deref().expect("--from was given");
    input
        .parse()
        .map_err(|e| format!("invalid version {input:?}: {e}"))
}

fn untagged_base() -> Version {
    UNTAGGED_BASE.parse().expect("valid literal")
}
