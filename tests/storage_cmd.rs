use nascli::commands::storage::resolve;

#[test]
fn maps_actions_to_methods() {
    assert_eq!(
        resolve("disks").unwrap(),
        ("ugreen.storage.grpc", "/storage.Storage/DiskList")
    );
    assert_eq!(
        resolve("volumes").unwrap(),
        ("ugreen.storage.grpc", "/storage.Storage/VolumeList")
    );
    assert_eq!(
        resolve("pools").unwrap(),
        ("ugreen.storage.grpc", "/storage.Storage/PoolList")
    );
    assert_eq!(
        resolve("smart").unwrap(),
        ("ugreen.storage.grpc", "/storage.Storage/AllDiskSmartInfo")
    );
}

#[test]
fn rejects_unknown_action() {
    assert!(resolve("nope").is_err());
}
