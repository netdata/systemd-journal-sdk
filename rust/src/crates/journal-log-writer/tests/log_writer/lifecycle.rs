use super::*;

#[test]
fn test_lifecycle_observer_reports_rotation_and_retention_deletion() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let config = Config::new(
        Origin {
            machine_id: Some(test_machine_id()),
            namespace: None,
            source: journal_registry::Source::System,
        },
        RotationPolicy::default().with_number_of_entries(1),
        RetentionPolicy::default().with_number_of_journal_files(1),
    );
    let observer = Arc::new(RecordingObserver::default());
    let mut log = Log::new(dir.path(), config)
        .expect("create log")
        .with_lifecycle_observer(observer.clone());

    log.write_entry(&[b"MESSAGE=one"], None)
        .expect("write first entry");
    log.write_entry(&[b"MESSAGE=two"], None)
        .expect("write second entry");
    log.write_entry(&[b"MESSAGE=three"], None)
        .expect("write third entry");

    let events = observer
        .events
        .lock()
        .expect("lock observer events")
        .clone();
    let rotation_count = events
        .iter()
        .filter(|event| matches!(event, LogLifecycleEvent::Rotated { .. }))
        .count();
    let deleted_files = events
        .iter()
        .find_map(|event| match event {
            LogLifecycleEvent::RetainedDeleted { files } => Some(files.clone()),
            _ => None,
        })
        .unwrap_or_default();

    assert_eq!(
        rotation_count, 2,
        "expected two rotations after three writes"
    );
    assert_eq!(deleted_files.len(), 1, "expected one retained deletion");
    assert!(
        !Path::new(deleted_files[0].path()).exists(),
        "retained file should be gone from disk: {}",
        deleted_files[0].path()
    );
}

#[test]
fn test_artifact_sizer_contributes_to_retention_bytes() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let config = Config::new(
        Origin {
            machine_id: Some(test_machine_id()),
            namespace: None,
            source: journal_registry::Source::System,
        },
        RotationPolicy::default().with_number_of_entries(1),
        RetentionPolicy::default().with_size_of_journal_files(1),
    );
    let observer = Arc::new(RecordingObserver::default());
    let sizer = Arc::new(FixedArtifactSizer::default());
    let mut log = Log::new(dir.path(), config)
        .expect("create log")
        .with_lifecycle_observer(observer.clone())
        .with_artifact_sizer(sizer.clone());

    log.write_entry(&[b"MESSAGE=artifact-retention-0"], None)
        .expect("write first entry");
    log.write_entry(&[b"MESSAGE=artifact-retention-1"], None)
        .expect("write second entry");

    assert!(
        !sizer.calls.lock().expect("lock artifact calls").is_empty(),
        "artifact sizer should be consulted during retention"
    );
    let events = observer.events.lock().expect("lock observer events");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, LogLifecycleEvent::Created { .. })),
        "first append should report active creation"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, LogLifecycleEvent::Rotated { .. })),
        "second append should rotate"
    );
    let deleted = events
        .iter()
        .find_map(|event| match event {
            LogLifecycleEvent::RetainedDeleted { files } => Some(files),
            _ => None,
        })
        .expect("artifact-inclusive retention should delete old archive");
    assert_eq!(deleted.len(), 1);
}

#[test]
fn test_lifecycle_observer_reports_missing_retention_deletions() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let config = Config::new(
        Origin {
            machine_id: Some(test_machine_id()),
            namespace: None,
            source: journal_registry::Source::System,
        },
        RotationPolicy::default().with_number_of_entries(1),
        RetentionPolicy::default().with_number_of_journal_files(1),
    );
    let observer = Arc::new(RecordingObserver::default());
    let mut log = Log::new(dir.path(), config)
        .expect("create log")
        .with_lifecycle_observer(observer.clone());

    log.write_entry(&[b"MESSAGE=one"], None)
        .expect("write first entry");
    log.write_entry(&[b"MESSAGE=two"], None)
        .expect("write second entry");

    let archived_path = journal_file_paths(&dir)
        .into_iter()
        .find(|path| path.to_string_lossy().contains('@'))
        .expect("archived path after first rotation");
    fs::remove_file(&archived_path).expect("remove archived file before retention");

    log.write_entry(&[b"MESSAGE=three"], None)
        .expect("write third entry");

    let events = observer.events.lock().expect("lock observer events");
    let retained = events
        .iter()
        .filter_map(|event| match event {
            LogLifecycleEvent::RetainedDeleted { files } => Some(files),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();

    assert!(
        retained
            .iter()
            .any(|file| Path::new(file.path()) == archived_path),
        "files removed from chain/accounting must still be reported for retention follow-up"
    );
}
