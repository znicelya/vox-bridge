use super::join::*;
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

#[tokio::test]
async fn audio_actual_parents_keep_two_stream_slots_and_inventory_independent_until_join() {
    let registry = AudioRegistry::new();
    let first = registry.stream().unwrap();
    let second = registry.stream().unwrap();
    let inventory = registry.inventory().await.unwrap();
    assert!(
        registry.stream().is_err(),
        "two original parents must prevent a third open"
    );
    let key = AudioCloseKey::new(1, Uuid::new_v4(), registry.epoch().unwrap());
    let close = registry.begin_close(key.clone()).unwrap();
    assert!(registry.stream().is_err());
    assert!(registry.inventory().await.is_err());
    first.finish().await.unwrap();
    second.finish().await.unwrap();
    let pending = tokio::time::timeout(Duration::from_millis(20), close.wait())
        .await
        .is_err();
    inventory.finish().await.unwrap();
    let ack = close.wait().await.unwrap();
    assert!(
        pending,
        "stream completion must not acknowledge a held inventory parent"
    );
    registry.reopen_after_close(&ack).unwrap();
    let stale = registry.reopen_after_close(&ack);
    assert!(
        stale.is_err(),
        "a prior actual receipt must not rearm a newer backend epoch"
    );
    registry.stream().unwrap().finish().await.unwrap();
}

#[tokio::test]
async fn audio_one_shared_helper_group_keeps_held_leaf_across_new_poll() {
    let registry = AudioRegistry::new();
    let parent = registry.stream().unwrap();
    let helpers = parent.helpers();
    let group = helpers.begin().unwrap();
    let (source, actual) = group.leaf().unwrap().enter().unwrap();
    let pending = group.join();
    tokio::pin!(pending);
    let held = tokio::time::timeout(Duration::from_millis(20), &mut pending)
        .await
        .is_err();
    assert!(
        helpers.begin().is_err(),
        "an old helper must keep the original association group across ticks"
    );
    source.exited(Some(0)).await;
    actual.wait().await.unwrap();
    pending.await.unwrap();
    let next = helpers.begin().unwrap();
    next.join().await.unwrap();
    parent.finish().await.unwrap();
    assert!(held);
}

#[tokio::test]
async fn audio_actual_close_rejects_stale_key_and_waits_preentered_source() {
    let registry = Arc::new(AudioRegistry::new());
    let parent = registry.inventory().await.unwrap();
    let key = AudioCloseKey::new(9, Uuid::new_v4(), registry.epoch().unwrap());
    let first = registry.begin_close(key.clone()).unwrap();
    assert!(
        registry
            .begin_close(AudioCloseKey::new(10, Uuid::new_v4(), key.backend_epoch()))
            .is_err()
    );
    let same = registry.begin_close(key).unwrap();
    let a = tokio::time::timeout(Duration::from_millis(20), first.wait()).await;
    parent.finish().await.unwrap();
    assert!(a.is_err());
    same.wait().await.unwrap();
}

#[tokio::test]
async fn audio_finished_parent_cannot_enter_another_helper_group() {
    let registry = AudioRegistry::new();
    let parent = registry.stream().unwrap();
    let old_helpers = parent.helpers();
    old_helpers.begin().unwrap().join().await.unwrap();
    parent.finish().await.unwrap();
    let late = old_helpers.begin();
    let accepted = late.is_ok();
    if let Ok(scope) = late {
        scope.join().await.unwrap();
    }
    assert!(
        !accepted,
        "a retired actual parent must not repopulate its backend generation"
    );
}
