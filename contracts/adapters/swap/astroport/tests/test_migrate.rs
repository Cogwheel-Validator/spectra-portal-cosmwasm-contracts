use cosmwasm_std::{
    testing::{mock_dependencies, mock_env},
    Addr,
};
use cw2::{get_contract_version, set_contract_version};
use skip::swap::MigrateMsg;
use skip_go_swap_adapter_astroport::{contract::migrate, state::ENTRY_POINT_CONTRACT_ADDRESS};

#[test]
fn test_migrate_sets_version_and_entry_point() {
    let mut deps = mock_dependencies();
    set_contract_version(
        deps.as_mut().storage,
        "skip-go-swap-adapter-astroport",
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

    let version = get_contract_version(&deps.storage).unwrap();
    assert_eq!(version.contract, "skip-go-swap-adapter-astroport");
    assert_eq!(version.version, env!("CARGO_PKG_VERSION"));

    assert_eq!(
        ENTRY_POINT_CONTRACT_ADDRESS.load(&deps.storage).unwrap(),
        Addr::unchecked("entry_point")
    );
    assert_eq!(res.attributes[0].value, "migrate");
}

// NOTE: migrate does not check the stored cw2 contract name before overwriting.
#[test]
fn test_migrate_does_not_check_contract_name() {
    let mut deps = mock_dependencies();
    set_contract_version(deps.as_mut().storage, "unrelated-contract", "5.0.0").unwrap();

    assert!(migrate(
        deps.as_mut(),
        mock_env(),
        MigrateMsg {
            entry_point_contract_address: "entry_point".to_string(),
        },
    )
    .is_ok());
}
