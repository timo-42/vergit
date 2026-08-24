# vergit

Generate a [PEP 440](https://peps.python.org/pep-0440/) version string from git
state. A small Rust CLI over a dependency-free library, plus GitHub and Gitea
actions.

```console
$ vergit pep440                  # what version is this commit?
1.2.4.dev5+g1a2b3c4

$ vergit pep440 --next-minor     # what should I tag next?
1.3.0
```

## Two modes

**Describe** (default) answers *what version is this commit*. It is what CI
should stamp on a build.

| Git state | Output |
| --- | --- |
| exactly on tag `v1.2.3`, clean | `1.2.3` |
| exactly on tag, dirty | `1.2.3+g1a2b3c4.dirty` |
| `v1.2.3` + 5 commits | `1.2.4.dev5+g1a2b3c4` |
| `v1.2.3` + 5 commits, dirty | `1.2.4.dev5+g1a2b3c4.dirty` |
| no tags at all | `0.0.0.dev42+g1a2b3c4` (42 commits) |

A clean checkout of a tag reproduces that tag byte for byte. Everything else is
a developmental release of the *next* version, so it sorts after the tag it came
from and before the release it is heading for:

```
1.2.3  <  1.2.3+g1a2b3c4.dirty  <  1.2.4.dev5+g1a2b3c4  <  1.2.4
```

**Next** (`--next-*`) answers *what should I tag next*. Output is a bare,
taggable version — never `.devN`, never `+local`, regardless of how many commits
have landed or whether the tree is dirty.

```console
$ vergit pep440 --next-patch                  # from tag v1.2.3
1.2.4
$ vergit pep440 --next-minor --next-alpha
1.3.0a1
$ git tag -a "$(vergit pep440 --next-minor --with-prefix)" -m release
```

`--next-X` means **the smallest X-level release that sorts strictly above the
current version**. Because a pre-release is a candidate *for* its release,
finalizing one falls out of the same rule rather than needing its own flag:

| current tag | `--next-patch` | `--next-minor` | `--next-major` |
| --- | --- | --- | --- |
| `1.2.3` | `1.2.4` | `1.3.0` | `2.0.0` |
| `1.2.4rc1` | `1.2.4` | `1.3.0` | `2.0.0` |
| `1.3.0rc1` | `1.3.0` | `1.3.0` | `2.0.0` |

One release level and one phase may be combined — they are independent axes.
Phases only move forward (`a` → `b` → `rc` → final); asking to go backwards is
an error rather than output that sorts the wrong way.

| current tag | flags | result |
| --- | --- | --- |
| `1.2.3` | `--next-minor --next-alpha` | `1.3.0a1` |
| `1.2.4a1` | `--next-alpha` | `1.2.4a2` |
| `1.2.4a1` | `--next-beta` | `1.2.4b1` |
| `1.2.4b2` | `--next-alpha` | error: cannot go back from phase b to a |
| `1.2.3` | `--next-alpha` | error: needs a release level too |

That last error is deliberate: an alpha of `1.2.3` is in the past, and `1.2.4a1`,
`1.3.0a1` and `2.0.0a1` are all equally plausible, so the tool asks rather than
guesses. Pair it with a release level.

## Which tag gets used

Tags matching `--tag-prefix` that are not valid PEP 440 versions — `vlatest`,
`vstable` — are **ignored**, and the search continues past them.

When several version tags sit on one commit, **the highest version wins**. This
is deliberately not what `git describe` does; its documented tie-break is
"annotated tags will always be preferred over lightweight tags, and tags with
newer dates will always be preferred over tags with older dates", which means the
answer would otherwise depend on how a tag was created rather than on what it
says. With `v1.2.4rc1` and `v1.3.0` on the same commit, this reports `1.3.0`
whichever one is annotated.

Both modes work from the **highest version tag reachable from HEAD** — never the
nearest one. Describe mode then counts commits from that tag for its `.devN`.

Nearest is the obvious choice and it is wrong, because history merges. Merge a
`v1.0.1` hotfix branch back into a main that has already released `v2.0.0` and
the hotfix tag is now the *nearest* tag to HEAD:

```
*   merge hotfix        <- HEAD
|\
| * hotfix              <- v1.0.1   (nearest)
* | c7, c6, c5
* | c4                  <- v2.0.0   (highest reachable)
|/
* c1                    <- v1.0.0
```

`git describe` reports `v1.0.1` here. Measuring from it would stamp this build
`1.0.2.dev7` — **below `2.0.0`, a release the build already contains.** Measuring
from the highest reachable tag gives `2.0.1.dev5`, which sorts above every
release in its history and below the release it is heading for.

Restricting to *reachable* tags is what keeps maintenance branches in their own
series rather than jumping to the mainline version:

```
main:        v1.0.0, v2.0.0        on main, --next-patch -> 2.0.1
1.x branch:  v1.0.0, v1.0.1        on 1.x,  --next-patch -> 1.0.2
```

The one exception is a tag on HEAD itself, which always wins so that a release
checkout reproduces exactly what was tagged.

## Usage

```
vergit pep440 [options]

      --next-major       Next major release (X.0.0)
      --next-minor       Next minor release (x.Y.0)
      --next-patch       Next patch release (x.y.Z)
      --next-alpha       ...as an alpha (aN)
      --next-beta        ...as a beta (bN)
      --next-rc          ...as a release candidate (rcN)
      --with-prefix      Prepend the tag prefix, ready for `git tag`
      --from <STR>       Use STR as the current version instead of reading git
      --check            With --from, exit 0 if STR is already canonical, 1 if not
      --no-local         Omit the +local label (required by public indexes)
      --format <FORMAT>  Output format: pep440 or docker [default: pep440]
      --separator <CHAR> Docker local-version separator: ., - or _ [default: .]
      --tag-prefix <P>   Tag prefix to match and strip [default: v]
  -C <DIR>               Run git in DIR
  -h, --help / -V, --version
```

`--from` replaces git entirely, so the tool doubles as a normalizer and
validator:

```console
$ vergit pep440 --from "  V1.0-RC-1  "
1.0rc1
$ vergit pep440 --from 1.2.3 --next-minor
1.3.0
```

Exit codes: `0` success, `1` `--check` found a non-canonical version, `2` usage
or environment error.

### Publishing to a package index

PyPI and other public indexes must reject local version labels, so pass
`--no-local` when the version is going to be uploaded:

```console
$ vergit pep440 --no-local
1.2.4.dev5
```

### Tagging a Docker image

Docker tags do not allow PEP 440's `+` local-version separator. The `docker`
output format preserves the commit and dirty metadata while rendering a valid
Docker tag:

```console
$ vergit pep440 --format=docker
1.2.4.dev5.g1a2b3c4
$ vergit pep440 --format=docker --separator=-
1.2.4.dev5-g1a2b3c4
$ docker build -t "example/app:$(vergit pep440 --format=docker)" .
```

The separator can be `.`, `-` or `_` and defaults to `.`. It replaces only the
boundary before the local label; dots within that label remain dots. PEP 440
epochs are encoded without the Docker-incompatible `!`, so `2!1.0+abc` becomes
`epoch2.1.0.abc`. Output is rejected if the final tag contains an invalid
character or exceeds Docker's 128-byte limit.

## Actions

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0        # tags are invisible in a shallow clone

- uses: timo-42/vergit-action@v1
  id: version

- run: echo "building ${{ steps.version.outputs.version }}"
```

Inputs: `tool-version`, `tool-repository`, `tag-prefix`, `no-local`, `next`,
`next-phase`, `with-prefix`, `fetch-tags`, `working-directory`. Output:
`version`.

```yaml
- uses: timo-42/vergit-action@v1
  id: next
  with:
    next: minor
    next-phase: rc
    with-prefix: 'true'
- run: git tag -a "${{ steps.next.outputs.version }}" -m release
```

**`fetch-depth: 0` is the single most common failure.** `actions/checkout`
clones to depth 1 by default, which hides every tag and would silently produce
`0.0.0.dev1`. The tool refuses instead, and the action's `fetch-tags` input
(on by default) deepens the checkout first.

See [vergit-action](https://github.com/timo-42/vergit-action) for the complete
action documentation.

## Layout

```
crates/pep440/       Library: parse, normalize, render, compare. Zero deps.
crates/version-cli/  The `vergit` binary. Only dep: pep440.
scripts/             Differential test against Python's `packaging`.
```

`pep440` knows nothing about git, and the CLI holds all the git logic. A second
version format would be a sibling crate behind a new subcommand.

## Binary size

Size is treated as a feature, ahead of runtime speed. The release binary is
around **330 KB**, and CI fails if it passes 600 KB.

That budget is why there is no `clap` (argument parsing is a `match` over
`std::env::args`), no `regex` (the parser is a hand-written byte scanner), and
no `gix` or `libgit2` (git state comes from shelling out to `git`, which is
present anywhere this runs). The release profile uses `opt-level = "z"`, fat
LTO, one codegen unit, `panic = "abort"` and stripped symbols.

If you need it smaller, nightly `-Z build-std=std,panic_abort` with
`panic_immediate_abort` typically lands under 150 KB, at the cost of a nightly
toolchain.

## Correctness

`cargo test` covers the PEP's full ordering example, its normalization rules,
and the CLI end to end against real throwaway repositories.

Beyond that, `scripts/differential_test.py` checks the library against Python's
`packaging` — the reference implementation — over a ~30k-version corpus of valid
spellings, odd spellings and junk, comparing both normalization and sort order.
They agree on every input, with one documented exception: numeric segments are
`u64` rather than arbitrary precision, so a version above `u64::MAX` is rejected
rather than truncated.

```console
$ pip install packaging && python3 scripts/differential_test.py
corpus: 30752 version strings
documented divergences: 1/1 still rejected
normalize: 30751/30751 agree
ordering: 30731/30731 agree

OK: pep440 matches packaging on normalization and ordering
```

## Why `.devN` and not `.postN`

PEP 440 defines developmental releases as versions "created directly from source
control, typically for continuous integration purposes" — exactly this use case.
Post-releases are for minor errors that do not affect the distributed software,
such as correcting release notes, and the PEP says "the use of post-releases to
publish maintenance releases containing actual bug fixes is strongly
discouraged". Commits after a tag are real changes, so `.postN` would misuse the
segment. It is not implemented.

The local label (`+g1a2b3c4`) is permitted for "private builds created directly
from the project source"; the PEP's restriction is that public index servers
must reject local versions, hence `--no-local`.

## License

MIT
