#![allow(deprecated)]

use cosmwasm_std::testing::{mock_dependencies, mock_env};
use cw2::{get_contract_version, set_contract_version};
use skip2::ibc::MigrateMsg;
use skip_go_ibc_adapter_ibc_callbacks::{contract::migrate, state::ENTRY_POINT_CONTRACT_ADDRESS};

#[test]
fn test_migrate_sets_version_and_entry_point() {
    let mut deps = mock_dependencies();
    let entry_point = deps.api.addr_make("entry_point");

    set_contract_version(
        deps.as_mut().storage,
        "skip-go-ibc-adapter-ibc-callbacks",
        "0.1.0",
    ).unwrap();

    migrate(
        deps.as_mut(),
        mock_env(),
        MigrateMsg {
            entry_point_contract_address: entry_point.to_string(),
        },
    )
    .unwrap();

    let version = get_contract_version(&deps.storage).unwrap();
    assert_eq!(version.contract, "skip-go-ibc-adapter-ibc-callbacks");
    assert_eq!(version.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(
        ENTRY_POINT_CONTRACT_ADDRESS.load(&deps.storage).unwrap(),
        entry_point
    );
}

#[test]
fn test_migrate_rejects_invalid_entry_point_address() {
    let mut deps = mock_dependencies();
    // A non-bech32 string is not a valid address under the 2.x mock API.
    let res = migrate(
        deps.as_mut(),
        mock_env(),
        MigrateMsg {
            entry_point_contract_address: "not-a-bech32-address".to_string(),
        },
    );
    assert!(res.is_err());
}
