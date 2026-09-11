#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::{fs::symlink, fs::PermissionsExt, net::UnixListener},
    };

    use tempfile::TempDir;

    use super::{
        canonical_source_directory, delete_quarantined_skill, digest_tree,
        digest_tree_with_root_identity, finalize_skill_import, inspect_central_skill,
        prepare_skill_import, quarantine_central_skill, read_central_skill_for_adoption,
    };
    use crate::{app::AppPaths, domain::SkillStatus};

    struct Fixture {
        _temporary: TempDir,
        paths: AppPaths,
        root: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let root = fs::canonicalize(temporary.path()).unwrap();
            let paths = AppPaths::from_data_root(root.join("private/app-data")).unwrap();
            paths.initialize().unwrap();
            Self {
                _temporary: temporary,
                paths,
                root,
            }
        }

        fn source(&self, name: &str) -> std::path::PathBuf {
            let source = self.root.join(name);
            fs::create_dir(&source).unwrap();
            source
        }
    }

    fn write_valid_skill(source: &std::path::Path, name: &str) {
        fs::write(
            source.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: 隔离测试 Skill\n---\n\n# {name}\n"),
        )
        .unwrap();
        fs::create_dir(source.join("scripts")).unwrap();
        fs::write(source.join("scripts/run.sh"), "#!/bin/sh\necho fixture\n").unwrap();
    }

    #[test]
    fn valid_import_is_staged_hashed_and_atomically_copied_without_touching_source() {
        let fixture = Fixture::new();
        let source = fixture.source("source");
        write_valid_skill(&source, "fixture-skill");
        fs::set_permissions(
            source.join("scripts/run.sh"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        symlink("scripts/run.sh", source.join("runner")).unwrap();
        let before_skill = fs::read(source.join("SKILL.md")).unwrap();
        let before_script = fs::read(source.join("scripts/run.sh")).unwrap();

        let mut prepared = prepare_skill_import(&fixture.paths, &source).unwrap();
        assert_eq!(prepared.name, "fixture-skill");
        assert_eq!(prepared.content_hash.len(), 64);
        assert_eq!(
            prepared.content_hash,
            digest_tree(&source, None).unwrap().hash
        );
        finalize_skill_import(&fixture.paths, &mut prepared).unwrap();

        assert_eq!(fs::read(source.join("SKILL.md")).unwrap(), before_skill);
        assert_eq!(
            fs::read(source.join("scripts/run.sh")).unwrap(),
            before_script
        );
        assert!(source.join("runner").is_symlink());
        let central = std::path::Path::new(&prepared.central_path);
        assert_eq!(
            central.file_name().and_then(|name| name.to_str()),
            Some("fixture-skill"),
            "中央副本目录必须以 frontmatter.name 命名"
        );
        assert_eq!(
            prepared.content_hash,
            digest_tree(central, None).unwrap().hash
        );
        assert_eq!(fs::read(central.join("SKILL.md")).unwrap(), before_skill);
        assert!(central.join("runner").is_symlink());
        assert_eq!(
            fs::metadata(central).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(central.join("scripts"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(central.join("SKILL.md"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(central.join("scripts/run.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(source.join("scripts/run.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755,
            "导入不得修改来源权限"
        );
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn same_name_import_conflicts_without_leaving_partial_copies() {
        let fixture = Fixture::new();
        let first = fixture.source("first");
        write_valid_skill(&first, "fixture-skill");
        let mut prepared = prepare_skill_import(&fixture.paths, &first).unwrap();
        finalize_skill_import(&fixture.paths, &mut prepared).unwrap();

        // 来源目录名与 frontmatter.name 不同：中央命名只看 frontmatter.name。
        let second = fixture.source("second");
        write_valid_skill(&second, "fixture-skill");
        assert!(prepare_skill_import(&fixture.paths, &second).is_err());
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());
        assert_eq!(
            fs::read_dir(fixture.paths.central_skills())
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn missing_or_malformed_frontmatter_is_rejected_and_staging_is_cleaned() {
        for (name, content) in [
            ("missing", "# no frontmatter\n"),
            ("broken-yaml", "---\nname: [\ndescription: broken\n---\n"),
            ("missing-description", "---\nname: fixture-skill\n---\n"),
            (
                "unsafe-name",
                "---\nname: ../escape\ndescription: bad\n---\n",
            ),
            (
                "empty-body",
                "---\nname: empty-body\ndescription: bad\n---\n\n",
            ),
            (
                "reserved-name",
                "---\nname: synced\ndescription: reserved\n---\n\n# body\n",
            ),
        ] {
            let fixture = Fixture::new();
            let source = fixture.source(name);
            fs::write(source.join("SKILL.md"), content).unwrap();
            assert!(prepare_skill_import(&fixture.paths, &source).is_err());
            assert!(fs::read_dir(fixture.paths.staging())
                .unwrap()
                .next()
                .is_none());
            assert!(fs::read_dir(fixture.paths.central_skills())
                .unwrap()
                .next()
                .is_none());
        }
    }

    #[test]
    fn escape_broken_cycle_directory_links_and_special_files_are_rejected() {
        for case in ["escape", "broken", "cycle", "directory", "special"] {
            let fixture = Fixture::new();
            let source = fixture.source(case);
            write_valid_skill(&source, "fixture-skill");
            match case {
                "escape" => {
                    fs::write(fixture.root.join("outside.txt"), "outside").unwrap();
                    symlink("../outside.txt", source.join("bad-link")).unwrap();
                }
                "broken" => symlink("missing.txt", source.join("bad-link")).unwrap(),
                "cycle" => {
                    symlink("loop-b", source.join("loop-a")).unwrap();
                    symlink("loop-a", source.join("loop-b")).unwrap();
                }
                "directory" => symlink("scripts", source.join("linked-directory")).unwrap(),
                "special" => {
                    UnixListener::bind(source.join("socket")).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                prepare_skill_import(&fixture.paths, &source).is_err(),
                "{case}"
            );
            assert!(source.exists(), "来源目录不能被删除：{case}");
            assert!(fs::read_dir(fixture.paths.staging())
                .unwrap()
                .next()
                .is_none());
        }
    }

    #[test]
    fn source_root_symlink_and_missing_skill_md_are_rejected() {
        let fixture = Fixture::new();
        let source = fixture.source("real");
        let alias = fixture.root.join("alias");
        symlink(&source, &alias).unwrap();
        assert!(prepare_skill_import(&fixture.paths, &alias).is_err());
        assert!(prepare_skill_import(&fixture.paths, &source).is_err());
    }

    #[test]
    fn oversized_and_unreadable_content_is_rejected_without_partial_copies() {
        let fixture = Fixture::new();
        let oversized = fixture.source("oversized");
        write_valid_skill(&oversized, "oversized-skill");
        let large = fs::File::create(oversized.join("large.bin")).unwrap();
        large.set_len(8 * 1024 * 1024 + 1).unwrap();
        assert!(prepare_skill_import(&fixture.paths, &oversized).is_err());

        let unreadable = fixture.source("unreadable");
        write_valid_skill(&unreadable, "unreadable-skill");
        let unreadable_file = unreadable.join("asset.txt");
        fs::write(&unreadable_file, "private").unwrap();
        fs::set_permissions(&unreadable_file, fs::Permissions::from_mode(0o000)).unwrap();
        let result = prepare_skill_import(&fixture.paths, &unreadable);
        fs::set_permissions(&unreadable_file, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(result.is_err());
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn hard_linked_files_are_rejected_without_modifying_the_source() {
        let fixture = Fixture::new();
        let source = fixture.source("hard-link");
        write_valid_skill(&source, "hard-link-skill");
        fs::write(source.join("asset.txt"), "same inode").unwrap();
        fs::hard_link(source.join("asset.txt"), source.join("asset-alias.txt")).unwrap();

        assert!(prepare_skill_import(&fixture.paths, &source).is_err());
        assert_eq!(fs::read(source.join("asset.txt")).unwrap(), b"same inode");
        assert_eq!(
            fs::read(source.join("asset-alias.txt")).unwrap(),
            b"same inode"
        );
        assert!(fs::read_dir(fixture.paths.staging())
            .unwrap()
            .next()
            .is_none());
    }

    #[test]
    fn root_replacement_and_executable_mode_changes_are_detected_by_stable_hash() {
        let fixture = Fixture::new();
        let source = fixture.source("identity");
        write_valid_skill(&source, "identity-skill");
        let script = source.join("scripts/run.sh");
        let non_executable_hash = digest_tree(&source, None).unwrap().hash;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let executable_hash = digest_tree(&source, None).unwrap().hash;
        assert_ne!(non_executable_hash, executable_hash);

        let (canonical, identity) = canonical_source_directory(&source).unwrap();
        let moved = fixture.root.join("identity-original");
        fs::rename(&source, &moved).unwrap();
        fs::create_dir(&source).unwrap();
        write_valid_skill(&source, "replacement-skill");
        assert!(digest_tree_with_root_identity(&canonical, None, Some(identity)).is_err());
        assert!(moved.join("SKILL.md").is_file());
    }

    #[test]
    fn changed_quarantine_is_never_recursively_deleted() {
        let fixture = Fixture::new();
        let source = fixture.source("quarantine");
        write_valid_skill(&source, "quarantine-skill");
        let mut prepared = prepare_skill_import(&fixture.paths, &source).unwrap();
        finalize_skill_import(&fixture.paths, &mut prepared).unwrap();
        let quarantine = quarantine_central_skill(
            &fixture.paths,
            &prepared.id,
            &prepared.name,
            &prepared.central_path,
            &prepared.content_hash,
        )
        .unwrap()
        .unwrap();
        fs::write(quarantine.join("unknown.txt"), "external change").unwrap();

        assert!(
            delete_quarantined_skill(&fixture.paths, &quarantine, &prepared.content_hash).is_err()
        );
        assert!(quarantine.join("unknown.txt").is_file());
        assert!(!std::path::Path::new(&prepared.central_path).exists());
    }

    #[test]
    fn adoption_read_parses_drifted_skill_md_without_widening_inspect() {
        let fixture = Fixture::new();
        let source = fixture.source("adopt");
        write_valid_skill(&source, "adopt-skill");
        let mut prepared = prepare_skill_import(&fixture.paths, &source).unwrap();
        finalize_skill_import(&fixture.paths, &mut prepared).unwrap();
        fs::write(
            std::path::Path::new(&prepared.central_path).join("SKILL.md"),
            "---\nname: adopt-skill\ndescription: 漂移后的说明\n---\n\n# updated\n",
        )
        .unwrap();

        let inspection = inspect_central_skill(
            &fixture.paths,
            &prepared.id,
            &prepared.name,
            &prepared.central_path,
            &prepared.content_hash,
            SkillStatus::Ready,
            true,
        )
        .unwrap();
        assert_eq!(inspection.status, SkillStatus::Invalid);
        assert_eq!(
            inspection.diagnostic_code,
            Some("CENTRAL_SKILL_CONTENT_CHANGED")
        );
        assert_eq!(inspection.skill_md, None);

        let adopted = read_central_skill_for_adoption(
            &fixture.paths,
            &prepared.id,
            &prepared.name,
            &prepared.central_path,
        )
        .unwrap();
        assert_eq!(adopted.name, "adopt-skill");
        assert_eq!(
            adopted
                .frontmatter
                .get("description")
                .and_then(|value| value.as_str()),
            Some("漂移后的说明")
        );
        assert_ne!(adopted.content_hash, prepared.content_hash);
    }

    #[test]
    fn failed_import_cleanup_preserves_replaced_or_changed_operation_directories() {
        for replace in [false, true] {
            let fixture = Fixture::new();
            let source = fixture.source("source");
            write_valid_skill(&source, "one");
            let mut prepared = prepare_skill_import(&fixture.paths, &source).unwrap();
            finalize_skill_import(&fixture.paths, &mut prepared).unwrap();
            if replace {
                fs::rename(
                    &prepared.central_path,
                    fixture.root.join("preserved-original"),
                )
                .unwrap();
                fs::create_dir(&prepared.central_path).unwrap();
            }
            let sentinel = std::path::Path::new(&prepared.central_path).join("unknown.txt");
            fs::write(&sentinel, "preserve me").unwrap();
            assert!(super::cleanup_failed_import(&fixture.paths, &prepared).is_err());
            assert_eq!(fs::read_to_string(&sentinel).unwrap(), "preserve me");
        }
    }
    #[test]
    fn exclusive_finalize_rename_never_replaces_an_existing_directory() {
        let fixture = Fixture::new();
        let source = fixture.source("staged");
        let destination = fixture.source("existing");
        assert!(super::rename_import_exclusively(&source, &destination).is_err());
        assert!(source.is_dir());
        assert!(destination.is_dir());
    }
}
