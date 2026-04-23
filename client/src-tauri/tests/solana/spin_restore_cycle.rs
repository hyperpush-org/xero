//! Core spin-restore-spin acceptance test. Drives the supervisor + snapshot
//! store through three full cycles, making sure the single-active-cluster
//! invariant holds and that snapshot restore replays the same state.

use super::support::{fixture_with_scripted_launcher_and_fetcher, sample_record};
use cadence_desktop_lib::commands::solana::{AccountFetcher, ClusterKind, StartOpts};

pub fn spin_restore_cycle_runs_three_consecutive_times() {
    let fixture = fixture_with_scripted_launcher_and_fetcher();

    // Seed a stable account set that every restore should replay.
    fixture.fetcher.insert(sample_record("A", 1_000_000));
    fixture.fetcher.insert(sample_record("B", 2_000_000));
    fixture.fetcher.insert(sample_record("C", 3_000_000));

    for cycle in 0..3 {
        // Spin.
        let (handle, events) = fixture
            .state
            .supervisor()
            .start(ClusterKind::Localnet, StartOpts::default())
            .expect("supervisor should start on cycle");
        assert_eq!(handle.kind, ClusterKind::Localnet);
        assert!(!events.is_empty(), "events emitted on cycle {cycle}");

        // Snapshot.
        let meta = fixture
            .state
            .snapshots()
            .create(
                &format!("cycle-{cycle}"),
                "localnet",
                &handle.rpc_url,
                &["A".into(), "B".into(), "C".into()],
            )
            .expect("snapshot should be created");
        assert_eq!(meta.account_count, 3);

        // Restore — round-trip must be bit-identical because the fetcher
        // returns the same canned records.
        let manifest = fixture
            .state
            .snapshots()
            .read(&meta.id)
            .expect("manifest read");
        let replayed = fixture
            .fetcher
            .fetch(
                &manifest.rpc_url,
                &manifest
                    .accounts
                    .iter()
                    .map(|a| a.pubkey.clone())
                    .collect::<Vec<_>>(),
            )
            .expect("fetcher replay");
        assert!(
            cadence_desktop_lib::commands::solana::snapshot::verify_round_trip(
                &manifest, &replayed
            ),
            "snapshot restore must be bit-identical on cycle {cycle}"
        );

        // Down.
        fixture
            .state
            .supervisor()
            .stop()
            .expect("supervisor should stop cleanly");
        assert!(!fixture.state.supervisor().status().running);
    }

    // Three spins means launcher was asked to boot three times.
    assert_eq!(fixture.launcher.call_count(), 3);
}

pub fn snapshot_restore_is_bit_identical_across_process_boundary() {
    let fixture = fixture_with_scripted_launcher_and_fetcher();
    fixture.fetcher.insert(sample_record("A", 5_000));
    fixture.fetcher.insert(sample_record("B", 7_000));

    let (handle, _) = fixture
        .state
        .supervisor()
        .start(ClusterKind::Localnet, StartOpts::default())
        .unwrap();

    let meta = fixture
        .state
        .snapshots()
        .create(
            "baseline",
            "localnet",
            &handle.rpc_url,
            &["A".into(), "B".into()],
        )
        .unwrap();

    // Simulate a process restart: re-read the manifest from disk and
    // compare against a fresh fetch.
    let manifest = fixture.state.snapshots().read(&meta.id).unwrap();
    let replay = fixture
        .fetcher
        .fetch(&manifest.rpc_url, &["A".into(), "B".into()])
        .unwrap();
    assert!(
        cadence_desktop_lib::commands::solana::snapshot::verify_round_trip(&manifest, &replay),
        "round-trip must still be identical after a simulated restart"
    );

    // If we mutate the live fetcher state, the restore check should catch it.
    fixture.fetcher.mutate("A", |record| {
        record.lamports += 1;
    });
    let replay = fixture
        .fetcher
        .fetch(&manifest.rpc_url, &["A".into(), "B".into()])
        .unwrap();
    assert!(
        !cadence_desktop_lib::commands::solana::snapshot::verify_round_trip(&manifest, &replay),
        "mutated account must fail the round-trip check"
    );
}

pub fn starting_second_cluster_replaces_the_first() {
    let fixture = fixture_with_scripted_launcher_and_fetcher();

    let (first, _) = fixture
        .state
        .supervisor()
        .start(ClusterKind::Localnet, StartOpts::default())
        .unwrap();
    let first_pid = first.pid;

    let (second, _) = fixture
        .state
        .supervisor()
        .start(ClusterKind::MainnetFork, StartOpts::default())
        .unwrap();
    assert_ne!(first_pid, second.pid, "second start must spawn a new child");

    let status = fixture.state.supervisor().status();
    assert!(status.running);
    assert_eq!(status.kind, Some(ClusterKind::MainnetFork));
    assert_eq!(fixture.launcher.call_count(), 2);
}
