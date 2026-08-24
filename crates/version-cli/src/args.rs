//! Hand-rolled argument parsing.
//!
//! `clap` would be several times the size of everything else in this binary
//! put together, and the flag surface here is a dozen lines of `match`.

use std::path::PathBuf;

use pep440::{Level, PreKind};

pub const HELP: &str = "\
vergit — generate a version string from git state

Usage: vergit pep440 [options]

Two modes. By default, describe the current commit as a developmental
release. With any --next-* flag, print the next version to tag instead.

Options:
      --next-major       Next major release (X.0.0)
      --next-minor       Next minor release (x.Y.0)
      --next-patch       Next patch release (x.y.Z)
      --next-alpha       ...as an alpha (aN)
      --next-beta        ...as a beta (bN)
      --next-rc          ...as a release candidate (rcN)
      --with-prefix      Prepend the tag prefix, ready for `git tag`
      --from <STR>       Use STR as the current version instead of reading git
      --check            With --from, exit 0 if STR is already canonical,
                         1 if not, and print nothing
      --no-local         Omit the +local label (required by public indexes)
      --format <FORMAT>  Output format: pep440 or docker [default: pep440]
      --separator <CHAR> Docker local-version separator: ., - or _ [default: .]
      --tag-prefix <P>   Tag prefix to match and strip [default: v]
  -C <DIR>               Run git in DIR
  -h, --help             Print this help
  -V, --version          Print this tool's own version

Describing the current commit, given tag v1.2.3:
  on the tag, clean          1.2.3
  on the tag, dirty          1.2.3+g1a2b3c4.dirty
  5 commits later            1.2.4.dev5+g1a2b3c4
  no tags at all             0.0.0.dev42+g1a2b3c4

Docker-compatible output:
  --format docker             1.2.4.dev5.g1a2b3c4
  --format docker --separator -
                              1.2.4.dev5-g1a2b3c4

Choosing the next tag, given tag v1.2.3:
  --next-patch               1.2.4
  --next-minor               1.3.0
  --next-minor --next-alpha  1.3.0a1
  --next-patch --with-prefix v1.2.4

One release level and one phase may be combined. A phase on its own only
works when a pre-release is already in flight, since otherwise there is no
saying which release it precedes.

  git tag -a \"$(vergit pep440 --next-minor --with-prefix)\"

Tag selection:
  * Tags matching --tag-prefix that are not PEP 440 versions are ignored.
  * Both modes work from the highest version tag reachable from HEAD, not
    the nearest one, so merging an old patch branch back into a released
    mainline cannot stamp a build below a release it contains.
  * Restricting to reachable tags keeps a maintenance branch in its own
    series instead of jumping to the mainline version.
  * A tag on HEAD itself always wins, so a release checkout reproduces
    exactly what was tagged.
";

pub struct Args {
    pub from: Option<String>,
    pub check: bool,
    pub no_local: bool,
    pub tag_prefix: String,
    pub dir: Option<PathBuf>,
    pub level: Option<Level>,
    pub phase: Option<PreKind>,
    pub with_prefix: bool,
    pub format: OutputFormat,
    pub separator: char,
    separator_given: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Pep440,
    Docker,
}

impl Args {
    /// Whether any `--next-*` flag was given, selecting next-tag mode.
    pub fn is_next(&self) -> bool {
        self.level.is_some() || self.phase.is_some()
    }
}

pub enum Parsed {
    Run(Args),
    Help,
    ToolVersion,
}

pub fn parse(argv: impl IntoIterator<Item = String>) -> Result<Parsed, String> {
    let mut argv = argv.into_iter();
    let mut args = Args {
        from: None,
        check: false,
        no_local: false,
        tag_prefix: "v".to_owned(),
        dir: None,
        level: None,
        phase: None,
        with_prefix: false,
        format: OutputFormat::Pep440,
        separator: '.',
        separator_given: false,
    };

    // The subcommand names the version format. PEP 440 is the only one so far,
    // but requiring it keeps room for siblings without a breaking change.
    match argv.next().as_deref() {
        Some("pep440") => {}
        Some("-h" | "--help") => return Ok(Parsed::Help),
        Some("-V" | "--version") => return Ok(Parsed::ToolVersion),
        Some(other) => return Err(format!("unknown command {other:?} (expected `pep440`)")),
        None => return Err("missing command (expected `pep440`)".to_owned()),
    }

    while let Some(arg) = argv.next() {
        // Accept both `--opt value` and `--opt=value`.
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => {
                (flag.to_owned(), Some(value.to_owned()))
            }
            _ => (arg, None),
        };
        let mut value = |flag: &str| match inline.clone().or_else(|| argv.next()) {
            Some(v) => Ok(v),
            None => Err(format!("{flag} needs a value")),
        };

        match flag.as_str() {
            "--from" => args.from = Some(value("--from")?),
            "--format" => args.format = parse_format(&value("--format")?)?,
            "--separator" => {
                args.separator = parse_separator(&value("--separator")?)?;
                args.separator_given = true;
            }
            "--tag-prefix" => args.tag_prefix = value("--tag-prefix")?,
            "-C" => args.dir = Some(PathBuf::from(value("-C")?)),
            "--check" => args.check = true,
            "--no-local" => args.no_local = true,
            "--with-prefix" => args.with_prefix = true,
            "--next-major" => set_level(&mut args, Level::Major, &flag)?,
            "--next-minor" => set_level(&mut args, Level::Minor, &flag)?,
            "--next-patch" => set_level(&mut args, Level::Patch, &flag)?,
            "--next-alpha" => set_phase(&mut args, PreKind::A, &flag)?,
            "--next-beta" => set_phase(&mut args, PreKind::B, &flag)?,
            "--next-rc" => set_phase(&mut args, PreKind::Rc, &flag)?,
            "-h" | "--help" => return Ok(Parsed::Help),
            "-V" | "--version" => return Ok(Parsed::ToolVersion),
            other => return Err(format!("unknown option {other:?} (try --help)")),
        }
    }

    validate(&args)?;
    Ok(Parsed::Run(args))
}

fn parse_format(value: &str) -> Result<OutputFormat, String> {
    match value {
        "pep440" => Ok(OutputFormat::Pep440),
        "docker" => Ok(OutputFormat::Docker),
        _ => Err(format!(
            "unknown output format {value:?} (expected `pep440` or `docker`)"
        )),
    }
}

fn parse_separator(value: &str) -> Result<char, String> {
    let mut chars = value.chars();
    let separator = chars
        .next()
        .filter(|_| chars.next().is_none())
        .ok_or_else(|| "--separator needs exactly one character: `.`, `-` or `_`".to_owned())?;
    match separator {
        '.' | '-' | '_' => Ok(separator),
        _ => Err("--separator must be one of `.`, `-` or `_`".to_owned()),
    }
}

/// Release level and pre-release phase are independent axes, so one of each may
/// be combined — but two of the same axis contradict.
fn set_level(args: &mut Args, level: Level, flag: &str) -> Result<(), String> {
    match args.level.replace(level) {
        Some(old) if old != level => Err(conflict(level_flag(old), flag)),
        _ => Ok(()),
    }
}

fn set_phase(args: &mut Args, phase: PreKind, flag: &str) -> Result<(), String> {
    match args.phase.replace(phase) {
        Some(old) if old != phase => Err(conflict(phase_flag(old), flag)),
        _ => Ok(()),
    }
}

fn conflict(a: &str, b: &str) -> String {
    format!("{a} and {b} cannot be combined (pick one)")
}

fn level_flag(level: Level) -> &'static str {
    match level {
        Level::Major => "--next-major",
        Level::Minor => "--next-minor",
        Level::Patch => "--next-patch",
    }
}

fn phase_flag(phase: PreKind) -> &'static str {
    match phase {
        PreKind::A => "--next-alpha",
        PreKind::B => "--next-beta",
        PreKind::Rc => "--next-rc",
    }
}

fn validate(args: &Args) -> Result<(), String> {
    if args.check {
        if args.from.is_none() {
            return Err(
                "--check requires --from (git-derived versions are always canonical)".to_owned(),
            );
        }
        if args.is_next() {
            return Err(
                "--check only validates --from, so it cannot be combined with --next-*".to_owned(),
            );
        }
        if args.format != OutputFormat::Pep440 {
            return Err("--check cannot be combined with --format docker".to_owned());
        }
    }
    // Next-tag output is a bare version to be tagged, so there is no local
    // label to suppress and no reason to accept a flag implying otherwise.
    if args.is_next() && args.no_local {
        return Err(
            "--no-local has no effect with --next-* (its output has no local label)".to_owned(),
        );
    }
    if args.with_prefix && !args.is_next() {
        return Err("--with-prefix only applies to --next-* output".to_owned());
    }
    if args.separator_given && args.format != OutputFormat::Docker {
        return Err("--separator requires --format docker".to_owned());
    }
    Ok(())
}
