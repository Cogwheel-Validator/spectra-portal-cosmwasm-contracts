use cosmwasm_std::testing::{mock_dependencies, mock_env};
use skip::swap::MigrateMsg;
use skip_go_swap_adapter_osmosis_poolmanager::contract::migrate;

// NOTE: the osmosis-poolmanager adapter's migrate entry point is
// `unimplemented!()` and panics. A deployed instance cannot be migrated.
#[test]
#[should_panic(expected = "not implemented")]
fn test_migrate_is_unimplemented() {
    let mut deps = mock_dependencies();
    let _ = migrate(
        deps.as_mut(),
        mock_env(),
        MigrateMsg {
            entry_point_contract_address: "entry_point".to_string(),
        },
    );
}
