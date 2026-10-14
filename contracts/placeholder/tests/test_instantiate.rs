use cosmwasm_std::{
    testing::{mock_dependencies, mock_env, mock_info},
    Empty,
};
use cw2::get_contract_version;

// The placeholder contract only reserves an address for a later migration.
// Its sole job at instantiate is to write the cw2 contract version so that a
// future migrate can identify it.

#[test]
fn test_instantiate_sets_cw2_version() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let info = mock_info("creator", &[]);

    let res =
        skip_go_placeholder::contract::instantiate(deps.as_mut(), env, info, Empty {}).unwrap();

    // Instantiate emits no messages and no attributes.
    assert!(res.messages.is_empty());
    assert!(res.attributes.is_empty());

    // cw2 version is stored with the crate name and version.
    let version = get_contract_version(&deps.storage).unwrap();
    assert_eq!(version.contract, "skip-go-placeholder");
    assert_eq!(version.version, env!("CARGO_PKG_VERSION"));
}
