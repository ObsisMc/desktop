use super::*;
use pretty_assertions::assert_eq;
use std::fs;
use std::os::windows::fs::OpenOptionsExt;

/// A real sharing violation explains successful recovery followed by another quarantine.
#[test]
fn windows_locked_skill_recovery_reconverges_only_after_reader_releases() {
    ora_logging::with_trace_logging(|| {
        let (directory, pool, workspace) = fixture();
        let original = b"---\nname: review\ndescription: Reviews changes\n---\noriginal\n";
        let updated = b"---\nname: review\ndescription: Reviews changes\n---\nupdated\n";
        let (repository, clock, target, worker) =
            converged_target(directory.path(), &pool, &workspace, original);
        let manifest = directory
            .path()
            .join("workspace")
            .join(".agents")
            .join("skills")
            .join("review")
            .join("SKILL.md");
        let reader = fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&manifest)
            .unwrap();
        SqliteSkillRepository::with_clock(pool.clone(), clock.clone())
            .replace_plugin_skills(
                &PluginId::new("official", "skill-pack").unwrap(),
                "1.1.0",
                &[skill_package(directory.path(), "skill-review-v2", updated)],
                /*updated_at*/ 150,
            )
            .unwrap();

        // Repeated clicks close the exact unchanged baseline, but cannot release another process's handle.
        for now in [160, 20_000] {
            clock.set(now);
            let outcomes = reconcile_due(&repository, &clock, &worker, now);
            assert_eq!(outcomes.len(), 1);
            let Err((failed_target, error)) = &outcomes[0] else {
                panic!("reader must block rename")
            };
            assert_eq!(failed_target, &target);
            let report = ora_logging::ErrorReport::from_error(error);
            assert!(report.chain().contains("backup_previous"));
            assert_eq!(fs::read(&manifest).unwrap(), original);
            clock.set(now + 10_001);
            assert_eq!(
                repository
                    .quarantine_unfinished_operations(LocalTimestamp::from_millis(now + 10_001))
                    .unwrap(),
                1
            );
            assert_eq!(
                recover_target(&repository, &clock, &target).unwrap(),
                RecoveryOutcome::Recovered {
                    target: target.clone(),
                    finalized: 0,
                    closed_without_effect: 1,
                }
            );
            assert_eq!(fs::read(&manifest).unwrap(), original);
        }

        drop(reader);
        clock.set(40_000);
        let outcomes = reconcile_due(&repository, &clock, &worker, 40_000);
        assert_eq!(outcomes.len(), 1);
        assert!(
            matches!(&outcomes[0], Ok((id, ReconcileOutcome::Mutated { .. })) if id == &target)
        );
        assert_eq!(fs::read(&manifest).unwrap(), updated);
        let status = repository.load_target_status(&target).unwrap().unwrap();
        assert_eq!(status.status.phase(), &TargetPhase::Current);
        assert_eq!(
            status.status.progress().ready(),
            status.status.progress().desired()
        );
    });
}
