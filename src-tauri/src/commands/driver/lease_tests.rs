use crate::audio::platform::inventory::{PlatformAudioBackend, driver_test_support};

#[test]
fn maintenance_rejects_active_and_quarantined_audio_ownership() {
    let backend = PlatformAudioBackend::new();
    let request = driver_test_support::seed(&backend);
    let lease = driver_test_support::claim(&backend, &request).unwrap();
    assert!(backend.begin_maintenance().is_err());
    lease.quarantine();
    lease.release();
    drop(lease);
    assert!(
        backend.begin_maintenance().is_err(),
        "terminal cleanup must not clear quarantine"
    );
}
#[test]
fn maintenance_denies_direct_open_and_invalidates_pre_mutation_inventory() {
    let backend = PlatformAudioBackend::new();
    let request = driver_test_support::seed(&backend);
    let maintenance = backend.begin_maintenance().unwrap();
    assert!(driver_test_support::claim(&backend, &request).is_err());
    assert!(backend.begin_maintenance().is_err());
    maintenance.complete();
    assert!(
        driver_test_support::claim(&backend, &request).is_err(),
        "fresh inventory is required even after acknowledged uninstall"
    );
    let fresh = driver_test_support::seed(&backend);
    assert_ne!(fresh.inventory_generation, request.inventory_generation);
    let lease = driver_test_support::claim(&backend, &fresh).unwrap();
    lease.release();
    backend.begin_maintenance().unwrap().complete();
}
#[test]
fn dropped_maintenance_keeps_uncertain_ownership() {
    let backend = PlatformAudioBackend::new();
    drop(backend.begin_maintenance().unwrap());
    assert!(backend.begin_maintenance().is_err());
}
