use cosmwasm_std::{
    testing::{mock_dependencies, mock_env},
    Addr,
};
use cw2::{get_contract_version, set_contract_version};
use skip::swap::MigrateMsg;
use skip_go_swap_adapter_hallswap::{contract::migrate, state::ENTRY_POINT_CONTRACT_ADDRESS};

// Migrate sets the cw2 version to the current crate version and (re)stores the
// entry point contract address.

#[test]
fn test_migrate_sets_version_and_entry_point() {
    let mut deps = mock_dependencies();

    // Simulate an older deployed version.
    set_contract_version(
        deps.as_mut().storage,
        "skip-go-swap-adapter-hallswap",
        "0.1.0",
    )
    .unwrap();

    let res = migrate(
        deps.as_mut(),
        mock_env(),
        MigrateMsg {
            entry_point_contract_address: "entry_point".to_string(),
        },
    )
    .unwrap();

    // Version is bumped to the current crate version.
    let version = get_contract_version(&deps.storage).unwrap();
    assert_eq!(version.contract, "skip-go-swap-adapter-hallswap");
    assert_eq!(version.version, env!("CARGO_PKG_VERSION"));

    // Entry point address is stored.
    let stored = ENTRY_POINT_CONTRACT_ADDRESS.load(&deps.storage).unwrap();
    assert_eq!(stored, Addr::unchecked("entry_point"));

    assert_eq!(res.attributes[0].value, "migrate");
}

// NOTE: this adapter's migrate does NOT verify the stored cw2 contract *name*
// before overwriting it (no `cw2::ensure_from_older_version` / name check), so
// migrating a different contract's state into this code path would silently
// succeed.
#[test]
fn test_migrate_does_not_check_contract_name() {
    let mut deps = mock_dependencies();

    // Pretend the stored contract is something else entirely.
    set_contract_version(deps.as_mut().storage, "some-other-contract", "9.9.9").unwrap();

    let res = migrate(
        deps.as_mut(),
        mock_env(),
        MigrateMsg {
            entry_point_contract_address: "entry_point".to_string(),
        },
    );

    // Current behavior: succeeds and overwrites the name.
    assert!(res.is_ok());
    let version = get_contract_version(&deps.storage).unwrap();
    assert_eq!(version.contract, "skip-go-swap-adapter-hallswap");
}
