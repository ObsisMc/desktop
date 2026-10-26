use super::*;
use ora_effect::*;
use pretty_assertions::assert_eq;
use std::os::windows::fs::OpenOptionsExt;
use std::time::Duration;
mod recording;

/// Creates an exact owned baseline and a second revision through the production adapter.
fn update_fixture() -> (tempfile::TempDir, EffectOperation, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let source = temp.path().join("source");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&source).unwrap();
    let manifest = b"---\nname: review\ndescription: Reviews changes\n---\nold\n";
    fs::write(source.join("SKILL.md"), manifest).unwrap();
    let resource = EffectResource {
        descriptor: VersionedResourceDescriptor::FilesystemDirectoryV1(
            FilesystemDirectoryDescriptor {
                workspace_root: workspace,
                relative_path: ResourcePath::parse(".agents/skills").unwrap(),
            },
        ),
        ..test_resource()
    };
    let identity = ManagedIdentity::random();
    let native = NativeResourceIdentity::parse("review").unwrap();
    let first_fingerprint = fingerprint(&source).unwrap();
    let create = SkillDirectoryResourceAdapter
        .prepare_operation(
            &resource,
            ReconcileAttemptId::random(),
            Generation::new(1),
            /*sequence*/ 0,
            PlannedMutation {
                managed_identity: identity.clone(),
                desired_effect: None,
                mutation: EffectMutation::Create,
                expected: ExactPreviousState::Missing,
                planned: ExactPlannedState::Present {
                    native_identity: native.clone(),
                    managed_identity: identity.clone(),
                    fingerprint: first_fingerprint.clone(),
                },
                input: Some(VersionedMaterializationInput::SkillDirectoryV1(test_input(
                    &source,
                ))),
            },
            LocalTimestamp::from_millis(1),
        )
        .unwrap()
        .operation;
    SkillDirectoryResourceAdapter.apply(&create).unwrap();
    fs::write(
        source.join("SKILL.md"),
        b"---\nname: review\ndescription: Reviews changes\n---\nnew\n",
    )
    .unwrap();
    let update = SkillDirectoryResourceAdapter
        .prepare_operation(
            &resource,
            ReconcileAttemptId::random(),
            Generation::new(2),
            /*sequence*/ 0,
            PlannedMutation {
                managed_identity: identity.clone(),
                desired_effect: None,
                mutation: EffectMutation::Update,
                expected: ExactPreviousState::Present {
                    native_identity: native.clone(),
                    managed_identity: identity.clone(),
                    fingerprint: first_fingerprint,
                },
                planned: ExactPlannedState::Present {
                    native_identity: native,
                    managed_identity: identity,
                    fingerprint: fingerprint(&source).unwrap(),
                },
                input: Some(VersionedMaterializationInput::SkillDirectoryV1(test_input(
                    &source,
                ))),
            },
            LocalTimestamp::from_millis(2),
        )
        .unwrap()
        .operation;
    let VersionedAdapterPlan::FilesystemDirectoryV1(plan) = update.payload() else {
        panic!("directory plan")
    };
    let target = plan.resource_root.join("review");
    (temp, update, target)
}

/// Exercises the production update swap while a Windows reader temporarily denies deletion.
#[test]
fn update_survives_transient_windows_directory_lock() {
    ora_logging::with_trace_logging(|| {
        let (_temp, update, target) = update_fixture();
        let reader = fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(target.join("SKILL.md"))
            .unwrap();
        // Keep the same lock that produces ERROR_ACCESS_DENIED alive until apply has begun.
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            drop(reader);
        });
        let result = SkillDirectoryResourceAdapter.apply(&update);
        release.join().unwrap();
        result.expect("a brief reader must not force the skill back into manual recovery");
        SkillDirectoryResourceAdapter.verify(&update).unwrap();
        assert_eq!(
            fs::read(target.join("SKILL.md")).unwrap(),
            b"---\nname: review\ndescription: Reviews changes\n---\nnew\n"
        );
    });
}

/// A long-lived reader leaves exact old content recoverable; releasing it permits the same intent.
#[test]
fn persistent_windows_lock_preserves_baseline_and_can_recover_after_release() {
    let recording = recording::Recording::default();
    ora_logging::with_recorded_trace_logging(recording.clone(), || {
        let (_temp, update, target) = update_fixture();
        let before = fs::read(target.join("SKILL.md")).unwrap();
        let reader = fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(target.join("SKILL.md"))
            .unwrap();
        let error = SkillDirectoryResourceAdapter
            .apply(&update)
            .expect_err("locked baseline");
        let report = ora_logging::ErrorReport::from_error(&error);
        assert!(
            report.chain().contains("backup_previous"),
            "{}",
            report.chain()
        );
        assert_eq!(fs::read(target.join("SKILL.md")).unwrap(), before);
        SkillDirectoryResourceAdapter
            .verify_expected(&update)
            .expect("old state remains provable");
        assert!(SkillDirectoryResourceAdapter.verify(&update).is_err());
        drop(reader);
        SkillDirectoryResourceAdapter
            .apply(&update)
            .expect("released reader permits update");
        SkillDirectoryResourceAdapter
            .verify(&update)
            .expect("exact new state");
        let events = recording.events();
        let failure = events
            .iter()
            .find(|event| {
                event.get("stage").map(String::as_str) == Some("apply")
                    && event.get("outcome").map(String::as_str) == Some("failure")
            })
            .expect("a failed mutation must have a diagnostic event");
        for (field, expected) in [
            ("operation_id", update.identity().as_str()),
            ("attempt_id", update.attempt().as_str()),
            ("resource_id", update.resource().as_str()),
            ("os_error", "5"),
        ] {
            assert_eq!(failure.get(field).map(String::as_str), Some(expected));
        }
        assert!(failure["error.chain"].contains("backup_previous"));
        assert!(
            events
                .iter()
                .any(
                    |event| event.get("stage").map(String::as_str) == Some("verify_expected")
                        && event.get("outcome").map(String::as_str) == Some("success")
                )
        );
        assert!(
            !format!("{events:?}").contains("description: Reviews changes"),
            "logs must not contain package bytes"
        );
    });
}

/// Supplies a local Resource independent of planner fixtures.
fn test_resource() -> EffectResource {
    EffectResource {
        identity: EffectResourceId::new("resource-1"),
        scope: EffectScopeId::Workspace(ora_domain::WorkspaceId::new("workspace-1")),
        resource_key: ResourceKey::parse("filesystem-directory:.agents/skills").unwrap(),
        adapter: ResourceAdapterIdentity::parse("ora/filesystem-directory").unwrap(),
        descriptor: VersionedResourceDescriptor::FilesystemDirectoryV1(
            FilesystemDirectoryDescriptor {
                workspace_root: PathBuf::new(),
                relative_path: ResourcePath::parse(".agents/skills").unwrap(),
            },
        ),
        format: MaterializationFormat::skill_directory_v1(),
        lifecycle: ResourceLifecycle::Active,
    }
}

/// Binds each revision to the exact fixture package bytes.
fn test_input(source: &std::path::Path) -> SkillMaterializationInput {
    SkillMaterializationInput {
        name: SkillName::parse("review").unwrap(),
        source: SkillSourceKey {
            source_kind: SkillSourceKind::Local,
            namespace: ora_domain::Namespace::local(),
            name: SkillName::parse("review").unwrap(),
        },
        package_root: source.to_path_buf(),
        skill_md_digest: Digest::sha256(&fs::read(source.join("SKILL.md")).unwrap()),
        package_fingerprint: fingerprint(source).unwrap(),
    }
}
