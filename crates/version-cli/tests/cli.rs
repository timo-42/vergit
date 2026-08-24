//! End-to-end tests driving the real binary against real throwaway repos.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_vergit");

/// A git repository in a temp directory, removed on drop.
struct Repo(PathBuf);

impl Repo {
    fn new(name: &str) -> Repo {
        let dir = std::env::temp_dir().join(format!("version-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let repo = Repo(dir);

        repo.git(&["init", "--quiet", "--initial-branch=main"]);
        // Keep the test independent of the machine's git config.
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "user.name", "Test"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.0.join(name), contents).expect("write file");
    }

    fn commit(&self, msg: &str) {
        self.write("file.txt", msg);
        self.git(&["add", "."]);
        self.git(&["commit", "--quiet", "-m", msg]);
    }

    fn short_hash(&self) -> String {
        format!("g{}", self.git(&["rev-parse", "--short=8", "HEAD"]))
    }

    /// An annotated tag. `git describe` prefers these over lightweight ones,
    /// which is exactly the preference that must not decide the version.
    fn tag_annotated(&self, name: &str) {
        self.git(&["tag", "-a", name, "-m", name]);
    }

    /// Commits a uniquely named file, so branches merge without conflicts.
    fn commit_unique(&self, name: &str) {
        self.write(&format!("{name}.txt"), name);
        self.git(&["add", "."]);
        self.git(&["commit", "--quiet", "-m", name]);
    }

    /// Runs the CLI in this repo, asserting success.
    fn version(&self, extra: &[&str]) -> String {
        let (out, code) = self.try_version(extra);
        assert_eq!(code, 0, "expected success, got: {out}");
        out
    }

    fn try_version(&self, extra: &[&str]) -> (String, i32) {
        run(&[&["pep440", "-C", self.0.to_str().unwrap()], extra].concat())
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Parses CLI output so versions compare by PEP 440 order rather than as text.
fn ver(s: &str) -> pep440::Version {
    s.parse().unwrap_or_else(|e| panic!("{s:?} from CLI: {e}"))
}

/// Runs the CLI outside any particular repo, returning `(stdout+stderr, code)`.
fn run(args: &[&str]) -> (String, i32) {
    let out = Command::new(BIN).args(args).output().expect("run version");
    let mut text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        text.push_str(err.trim());
    }
    (text, out.status.code().expect("exit code"))
}

#[test]
fn clean_checkout_of_a_tag_reproduces_the_tag() {
    let repo = Repo::new("on-tag");
    repo.commit("one");
    repo.git(&["tag", "v1.2.3"]);
    assert_eq!(repo.version(&[]), "1.2.3");
}

#[test]
fn a_dirty_tag_is_marked_in_the_local_label() {
    let repo = Repo::new("dirty-tag");
    repo.commit("one");
    repo.git(&["tag", "v1.2.3"]);
    repo.write("file.txt", "changed");

    let hash = repo.short_hash();
    assert_eq!(repo.version(&[]), format!("1.2.3+{hash}.dirty"));
}

#[test]
fn commits_past_a_tag_become_a_dev_release_of_the_next_patch() {
    let repo = Repo::new("after-tag");
    repo.commit("one");
    repo.git(&["tag", "v1.2.3"]);
    repo.commit("two");
    repo.commit("three");

    let hash = repo.short_hash();
    assert_eq!(repo.version(&[]), format!("1.2.4.dev2+{hash}"));

    repo.write("file.txt", "changed");
    assert_eq!(repo.version(&[]), format!("1.2.4.dev2+{hash}.dirty"));
}

#[test]
fn an_untagged_repo_stays_at_zero_and_counts_every_commit() {
    let repo = Repo::new("untagged");
    repo.commit("one");
    repo.commit("two");

    let hash = repo.short_hash();
    assert_eq!(repo.version(&[]), format!("0.0.0.dev2+{hash}"));
}

#[test]
fn no_local_drops_the_label_everywhere() {
    let repo = Repo::new("no-local");
    repo.commit("one");
    repo.git(&["tag", "v1.2.3"]);
    repo.commit("two");
    repo.write("other.txt", "untracked");

    assert_eq!(repo.version(&["--no-local"]), "1.2.4.dev1");
}

#[test]
fn every_emitted_version_is_canonical_and_ordered() {
    let repo = Repo::new("ordering");
    repo.commit("one");
    repo.git(&["tag", "v1.2.3"]);
    let tagged = repo.version(&[]);
    repo.commit("two");
    let dev = repo.version(&[]);

    for v in [&tagged, &dev] {
        let (_, code) = run(&["pep440", "--from", v, "--check"]);
        assert_eq!(code, 0, "{v} is not canonical");
    }
    // The dev build sorts after the tag it came from.
    assert!(tagged < dev, "{tagged} < {dev}");
}

#[test]
fn tag_prefix_is_configurable() {
    let repo = Repo::new("prefix");
    repo.commit("one");
    repo.git(&["tag", "release-2.0"]);

    // The default `v` prefix does not match, so this looks untagged.
    assert!(repo.version(&[]).starts_with("0.0.0.dev"));
    assert_eq!(repo.version(&["--tag-prefix", "release-"]), "2.0");
    // An empty prefix matches any tag and strips nothing.
    let repo = Repo::new("bare-prefix");
    repo.commit("one");
    repo.git(&["tag", "3.1.4"]);
    assert_eq!(repo.version(&["--tag-prefix", ""]), "3.1.4");
}

#[test]
fn a_tag_that_is_not_a_version_is_ignored_rather_than_fatal() {
    let repo = Repo::new("bad-tag");
    repo.commit("one");
    repo.git(&["tag", "vNOPE"]);

    // Tags that match the prefix but are not versions are invisible, so this
    // repository looks untagged rather than failing outright.
    let hash = repo.short_hash();
    assert_eq!(repo.version(&[]), format!("0.0.0.dev1+{hash}"));
}

#[test]
fn a_shallow_clone_fails_loudly_instead_of_guessing() {
    let origin = Repo::new("shallow-origin");
    origin.commit("one");
    origin.commit("two");

    let dir = std::env::temp_dir().join(format!("version-test-shallow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = Command::new("git")
        .args(["clone", "--depth", "1", "--quiet"])
        .arg(format!("file://{}", origin.0.display()))
        .arg(&dir)
        .output()
        .expect("run git clone");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (text, code) = run(&["pep440", "-C", dir.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(code, 2, "{text}");
    assert!(text.contains("shallow"), "{text}");
    assert!(text.contains("fetch-depth"), "{text}");
}

#[test]
fn outside_a_repository_it_fails_with_a_useful_message() {
    let dir = std::env::temp_dir().join(format!("version-test-norepo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");

    let (text, code) = run(&["pep440", "-C", dir.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(code, 2, "{text}");
    assert!(text.contains("not a git repository"), "{text}");
}

#[test]
fn from_normalizes_without_touching_git() {
    // Deliberately run from a directory that is not a repository.
    let (out, code) = run(&["pep440", "--from", "v1.0-RC-1"]);
    assert_eq!(code, 0);
    assert_eq!(out, "1.0rc1");

    assert_eq!(run(&["pep440", "--from=1.0+abc-1"]).0, "1.0+abc.1");
    assert_eq!(run(&["pep440", "--from", "1.0+abc", "--no-local"]).0, "1.0");
}

#[test]
fn docker_format_preserves_local_metadata_with_a_safe_separator() {
    let repo = Repo::new("docker-format");
    repo.commit("one");
    repo.git(&["tag", "v1.2.3"]);
    repo.commit("two");

    let hash = repo.short_hash();
    assert_eq!(
        repo.version(&["--format", "docker"]),
        format!("1.2.4.dev1.{hash}")
    );

    repo.write("file.txt", "dirty");
    assert_eq!(
        repo.version(&["--format=docker", "--separator", "-"]),
        format!("1.2.4.dev1-{hash}.dirty")
    );
}

#[test]
fn docker_format_normalizes_input_and_encodes_epochs() {
    assert_eq!(
        run(&["pep440", "--from", "V1.0-RC-1+ABC_1", "--format=docker"]).0,
        "1.0rc1.abc.1"
    );
    assert_eq!(
        run(&[
            "pep440",
            "--from",
            "2!1.0+abc",
            "--format=docker",
            "--separator=-",
        ])
        .0,
        "epoch2-1.0-abc"
    );
}

#[test]
fn docker_format_enforces_the_tag_grammar_and_length_limit() {
    let valid = format!("1.0+{}", "a".repeat(124));
    let too_long = format!("1.0+{}", "a".repeat(125));
    assert_eq!(run(&["pep440", "--from", &valid, "--format=docker"]).1, 0);
    let (out, code) = run(&["pep440", "--from", &too_long, "--format=docker"]);
    assert_eq!(code, 2);
    assert!(out.contains("maximum is 128"), "{out}");

    let (out, code) = run(&[
        "pep440",
        "--from",
        "1.0",
        "--next-patch",
        "--with-prefix",
        "--tag-prefix",
        "bad/",
        "--format=docker",
    ]);
    assert_eq!(code, 2);
    assert!(out.contains("not a valid Docker tag"), "{out}");
}

#[test]
fn docker_format_rejects_invalid_format_options() {
    for args in [
        vec!["pep440", "--from", "1.0", "--format", "oci"],
        vec!["pep440", "--from", "1.0", "--separator", "-"],
        vec![
            "pep440",
            "--from",
            "1.0",
            "--format=docker",
            "--separator",
            "+",
        ],
        vec![
            "pep440",
            "--from",
            "1.0",
            "--format=docker",
            "--separator",
            "..",
        ],
        vec!["pep440", "--from", "1.0", "--format=docker", "--check"],
    ] {
        assert_eq!(run(&args).1, 2, "{args:?} should be rejected");
    }
}

#[test]
fn next_prints_a_bare_taggable_version() {
    let repo = Repo::new("next");
    repo.commit("one");
    repo.git(&["tag", "v1.2.3"]);

    assert_eq!(repo.version(&["--next-patch"]), "1.2.4");
    assert_eq!(repo.version(&["--next-minor"]), "1.3.0");
    assert_eq!(repo.version(&["--next-major"]), "2.0.0");
    assert_eq!(repo.version(&["--next-minor", "--next-alpha"]), "1.3.0a1");
    assert_eq!(repo.version(&["--next-patch", "--with-prefix"]), "v1.2.4");
}

#[test]
fn next_ignores_commit_distance_and_dirt() {
    let repo = Repo::new("next-distance");
    repo.commit("one");
    repo.git(&["tag", "v1.2.3"]);
    assert_eq!(repo.version(&["--next-minor"]), "1.3.0");

    // Neither more commits nor a dirty tree changes which version comes next.
    repo.commit("two");
    repo.commit("three");
    repo.write("file.txt", "dirty");
    assert_eq!(repo.version(&["--next-minor"]), "1.3.0");
    assert_eq!(repo.version(&["--next-patch"]), "1.2.4");
}

#[test]
fn next_counts_from_the_tag_not_the_derived_dev_version() {
    let repo = Repo::new("next-from-tag");
    repo.commit("one");
    repo.git(&["tag", "v1.2.4rc1"]);
    repo.commit("two");

    // The dev derivation is already at rc2, but the next tag after rc1 is rc2,
    // not rc3.
    assert_eq!(
        repo.version(&[]),
        format!("1.2.4rc2.dev1+{}", repo.short_hash())
    );
    assert_eq!(repo.version(&["--next-rc"]), "1.2.4rc2");
    // Finalizing the candidate.
    assert_eq!(repo.version(&["--next-patch"]), "1.2.4");
}

#[test]
fn next_on_an_untagged_repo_starts_from_zero() {
    let repo = Repo::new("next-untagged");
    repo.commit("one");

    assert_eq!(repo.version(&["--next-patch"]), "0.0.1");
    assert_eq!(repo.version(&["--next-minor"]), "0.1.0");
    assert_eq!(repo.version(&["--next-major"]), "1.0.0");
}

#[test]
fn next_output_is_always_a_valid_tag_and_sorts_above_the_current_one() {
    let repo = Repo::new("next-valid");
    repo.commit("one");
    repo.git(&["tag", "v1.2.3"]);

    for flags in [
        vec!["--next-patch"],
        vec!["--next-minor"],
        vec!["--next-major"],
        vec!["--next-minor", "--next-beta"],
        vec!["--next-major", "--next-rc"],
    ] {
        let next = repo.version(&flags);
        assert_eq!(
            run(&["pep440", "--from", &next, "--check"]).1,
            0,
            "{flags:?} produced non-canonical {next}"
        );
        assert!(
            ver(&next) > ver("1.2.3"),
            "{flags:?} produced {next}, not above 1.2.3"
        );
        // It is accepted by git as a tag name.
        repo.git(&["tag", &format!("v{next}")]);
    }
}

#[test]
fn next_rejects_impossible_and_contradictory_requests() {
    // An alpha of which release?
    let (out, code) = run(&["pep440", "--from", "1.2.3", "--next-alpha"]);
    assert_eq!(code, 2);
    assert!(out.contains("needs a release level"), "{out}");

    // Phases cannot move backwards.
    let (out, code) = run(&["pep440", "--from", "1.2.4rc1", "--next-beta"]);
    assert_eq!(code, 2);
    assert!(out.contains("cannot go back"), "{out}");

    // One flag per axis.
    let (out, code) = run(&["pep440", "--from", "1.2.3", "--next-minor", "--next-patch"]);
    assert_eq!(code, 2);
    assert!(out.contains("cannot be combined"), "{out}");

    for args in [
        vec!["pep440", "--from", "1.2.3", "--next-rc", "--no-local"],
        vec!["pep440", "--with-prefix"],
        vec!["pep440", "--from", "1.2.3", "--check", "--next-patch"],
    ] {
        assert_eq!(run(&args).1, 2, "{args:?} should be rejected");
    }
}

#[test]
fn with_prefix_uses_the_configured_prefix() {
    let repo = Repo::new("next-prefix");
    repo.commit("one");
    repo.git(&["tag", "release-2.0.0"]);

    let flags = ["--tag-prefix", "release-", "--next-minor", "--with-prefix"];
    assert_eq!(repo.version(&flags), "release-2.1.0");
    // Without --with-prefix the output stays bare PEP 440.
    assert_eq!(repo.version(&flags[..3]), "2.1.0");
}

#[test]
fn the_highest_tag_on_a_commit_wins_over_the_newer_one() {
    let repo = Repo::new("multi-newer");
    repo.commit("one");
    // `git describe` prefers the newer tag, which would report 1.2.9.
    repo.git(&["tag", "v1.3.0"]);
    repo.git(&["tag", "v1.2.9"]);

    assert_eq!(repo.version(&[]), "1.3.0");
    assert_eq!(repo.version(&["--next-patch"]), "1.3.1");
}

#[test]
fn the_highest_tag_on_a_commit_wins_over_the_annotated_one() {
    let repo = Repo::new("multi-annotated");
    repo.commit("one");
    // `git describe` prefers the annotated tag, which would report 1.0.0.
    repo.tag_annotated("v1.0.0");
    repo.git(&["tag", "v1.3.0"]);

    assert_eq!(repo.version(&[]), "1.3.0");
    assert_eq!(repo.version(&["--next-patch"]), "1.3.1");
}

#[test]
fn a_prerelease_and_its_release_on_one_commit_resolve_to_the_release() {
    let repo = Repo::new("multi-pre");
    repo.commit("one");
    repo.tag_annotated("v1.2.4rc1");
    repo.git(&["tag", "v1.3.0"]);

    assert_eq!(repo.version(&[]), "1.3.0");
    // The dangerous case: bumping from rc1 would suggest 1.2.4, below v1.3.0.
    assert_eq!(repo.version(&["--next-patch"]), "1.3.1");
}

#[test]
fn non_version_tags_are_ignored_when_a_real_one_shares_the_commit() {
    let repo = Repo::new("junk-sibling");
    repo.commit("one");
    repo.git(&["tag", "v1.2.3"]);
    repo.tag_annotated("vlatest");

    assert_eq!(repo.version(&[]), "1.2.3");
    assert_eq!(repo.version(&["--next-minor"]), "1.3.0");
}

#[test]
fn a_commit_tagged_only_with_junk_is_skipped_and_the_distance_stays_right() {
    let repo = Repo::new("junk-only");
    repo.commit("one");
    repo.git(&["tag", "v1.2.3"]);
    repo.commit("two");
    repo.tag_annotated("vlatest");

    // vlatest is not a version, so the search continues back to v1.2.3 — one
    // commit away, not zero.
    let hash = repo.short_hash();
    assert_eq!(repo.version(&[]), format!("1.2.4.dev1+{hash}"));
    assert_eq!(repo.version(&["--next-patch"]), "1.2.4");
}

#[test]
fn a_repo_with_only_junk_tags_counts_as_untagged() {
    let repo = Repo::new("junk-all");
    repo.commit("one");
    repo.git(&["tag", "vlatest"]);
    repo.commit("two");
    repo.git(&["tag", "vstable"]);

    let hash = repo.short_hash();
    assert_eq!(repo.version(&[]), format!("0.0.0.dev2+{hash}"));
    assert_eq!(repo.version(&["--next-patch"]), "0.0.1");
}

#[test]
fn next_bumps_from_the_highest_reachable_tag_not_the_nearest() {
    let repo = Repo::new("next-highest");
    repo.commit("one");
    repo.git(&["tag", "v2.0.0"]);
    repo.commit("two");
    repo.git(&["tag", "v1.5.0"]);

    // The nearest tag is v1.5.0, but v2.0.0 is reachable and higher.
    assert_eq!(repo.version(&["--next-patch"]), "2.0.1");
    assert_eq!(repo.version(&["--next-minor"]), "2.1.0");
    // Describe mode still answers the other question — what version *is* this
    // commit — so it reports the tag actually sitting on HEAD.
    assert_eq!(repo.version(&[]), "1.5.0");
}

/// Merging an old patch branch back into a mainline that has released a higher
/// version leaves the patch tag *nearest* to HEAD. Measuring from the nearest
/// tag would stamp the build below a release it already contains.
#[test]
fn merging_a_patch_branch_back_does_not_regress_below_a_released_version() {
    let repo = Repo::new("merge-back");
    repo.commit_unique("c1");
    repo.git(&["tag", "v1.0.0"]);
    for c in ["c2", "c3", "c4"] {
        repo.commit_unique(c);
    }
    repo.git(&["tag", "v2.0.0"]);
    for c in ["c5", "c6", "c7"] {
        repo.commit_unique(c);
    }

    repo.git(&["checkout", "--quiet", "-b", "maint", "v1.0.0"]);
    repo.commit_unique("hotfix");
    repo.git(&["tag", "v1.0.1"]);
    // On the branch itself, the hotfix tag is what this commit is.
    assert_eq!(repo.version(&[]), "1.0.1");

    repo.git(&["checkout", "--quiet", "main"]);
    repo.git(&["merge", "--quiet", "--no-ff", "maint", "-m", "merge hotfix"]);

    // `git describe` may select either tag on different Git versions; its
    // tie-break rules are deliberately not part of this tool's semantics.

    // Measured from the highest reachable tag instead: c5, c6, c7, hotfix, merge.
    let hash = repo.short_hash();
    let got = repo.version(&[]);
    assert_eq!(got, format!("2.0.1.dev5+{hash}"));

    // The property that matters: above every release it contains.
    assert!(ver(&got) > ver("2.0.0"), "{got} must sort above 2.0.0");
    assert!(ver(&got) > ver("1.0.1"), "{got} must sort above 1.0.1");
    assert!(ver(&got) < ver("2.0.1"), "{got} must sort below 2.0.1");

    assert_eq!(repo.version(&["--next-patch"]), "2.0.1");
}

#[test]
fn next_respects_branches_so_maintenance_lines_keep_their_own_series() {
    let repo = Repo::new("next-branch");
    repo.commit("one");
    repo.git(&["tag", "v1.0.0"]);
    repo.commit("two");
    repo.git(&["tag", "v2.0.0"]);

    // A maintenance branch off v1.0.0 must not jump to the mainline version.
    repo.git(&["checkout", "--quiet", "-b", "maint-1.x", "v1.0.0"]);
    repo.commit("backport");
    repo.git(&["tag", "v1.0.1"]);

    assert_eq!(repo.version(&["--next-patch"]), "1.0.2");

    repo.git(&["checkout", "--quiet", "main"]);
    assert_eq!(repo.version(&["--next-patch"]), "2.0.1");
}

#[test]
fn next_output_always_sorts_above_every_reachable_tag() {
    let repo = Repo::new("next-above-all");
    repo.commit("one");
    repo.tag_annotated("v2.0.0");
    repo.git(&["tag", "v1.9.9"]);
    repo.commit("two");
    repo.git(&["tag", "v1.5.0"]);

    let reachable: Vec<pep440::Version> = repo
        .git(&["tag", "--list", "v*", "--merged", "HEAD"])
        .lines()
        .map(|t| ver(t.trim_start_matches('v')))
        .collect();
    assert!(!reachable.is_empty());

    for flags in [
        vec!["--next-patch"],
        vec!["--next-minor"],
        vec!["--next-major"],
        vec!["--next-minor", "--next-rc"],
    ] {
        let next = ver(&repo.version(&flags));
        for tag in &reachable {
            assert!(next > *tag, "{flags:?} gave {next}, not above tag {tag}");
        }
    }
}

#[test]
fn usage_errors_exit_two_and_check_failure_exits_one() {
    assert_eq!(run(&["pep440", "--from", "nope"]).1, 2);
    assert_eq!(run(&["pep440", "--nonsense"]).1, 2);
    assert_eq!(run(&["semver"]).1, 2);
    assert_eq!(run(&[]).1, 2);
    assert_eq!(run(&["pep440", "--check"]).1, 2);
    assert_eq!(run(&["pep440", "--from", "1.0", "--check"]).1, 0);
    assert_eq!(run(&["pep440", "--from", "V1.0", "--check"]).1, 1);
    assert_eq!(run(&["--help"]).1, 0);
}
