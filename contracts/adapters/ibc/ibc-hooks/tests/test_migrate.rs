use cosmwasm_std::testing::{mock_dependencies, mock_env};
use cw2::{get_contract_version, set_contract_version};
use skip::ibc::MigrateMsg;
use skip_go_ibc_adapter_ibc_hooks::{contract::migrate, state::ENTRY_POINT_CONTRACT_ADDRESS};

#[test]
fn test_migrate_sets_version_and_entry_point() {
    let mut deps = mock_dependencies();
    set_contract_version(
        deps.as_mut().storage,
        "skip-go-ibc-adapter-ibc-hooks",
        "0.1.0",
    )
    .unwrap();

    migrate(
        deps.as_mut(),
        mock_env(),
        MigrateMsg {
            entry_point_contract_address: "entry_point".to_string(),
        },
    )
    .unwrap();

    let version = get_contract_version(&deps.storage).unwrap();
    assert_eq!(version.contract, "skip-go-ibc-adapter-ibc-hooks");
    assert_eq!(version.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(
        ENTRY_POINT_CONTRACT_ADDRESS
            .load(&deps.storage)
            .unwrap()
            .as_str(),
        "entry_point"
    );
}

#[test]
fn test_migrate_does_not_check_contract_name() {
    let mut deps = mock_dependencies();
    set_contract_version(deps.as_mut().storage, "unrelated", "9.9.9").unwrap();
    assert!(migrate(
        deps.as_mut(),
        mock_env(),
        MigrateMsg {
            entry_point_contract_address: "entry_point".to_string(),
        },
    )
    .is_ok());
}
