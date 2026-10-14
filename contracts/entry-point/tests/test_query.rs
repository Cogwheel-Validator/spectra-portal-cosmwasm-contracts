use cosmwasm_std::{
    from_json,
    testing::{mock_dependencies, mock_env, mock_info},
    Addr,
};
use skip::{
    entry_point::{InstantiateMsg, QueryMsg},
    swap::SwapVenue,
};
use skip_go_entry_point::contract::{instantiate, query};

// Exercises the two query handlers of the entry point contract, which have no
// direct coverage otherwise.

fn instantiate_contract(
    deps: cosmwasm_std::DepsMut,
) {
    let info = mock_info("creator", &[]);
    let mut env = mock_env();
    env.contract.address = Addr::unchecked("entry_point");

    instantiate(
        deps,
        env,
        info,
        InstantiateMsg {
            swap_venues: vec![SwapVenue {
                name: "osmosis-poolmanager".to_string(),
                adapter_contract_address: "osmosis_adapter".to_string(),
            }],
            ibc_transfer_contract_address: "ibc_transfer_adapter".to_string(),
            hyperlane_transfer_contract_address: None,
        },
    )
    .unwrap();
}

#[test]
fn test_query_swap_venue_adapter_contract() {
    let mut deps = mock_dependencies();
    instantiate_contract(deps.as_mut());

    let addr: Addr = from_json(
        query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::SwapVenueAdapterContract {
                name: "osmosis-poolmanager".to_string(),
            },
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(addr, Addr::unchecked("osmosis_adapter"));
}

#[test]
fn test_query_unknown_swap_venue_errors() {
    let mut deps = mock_dependencies();
    instantiate_contract(deps.as_mut());

    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::SwapVenueAdapterContract {
            name: "does-not-exist".to_string(),
        },
    );

    assert!(res.is_err());
}

#[test]
fn test_query_ibc_transfer_adapter_contract() {
    let mut deps = mock_dependencies();
    instantiate_contract(deps.as_mut());

    let addr: Addr = from_json(
        query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::IbcTransferAdapterContract {},
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(addr, Addr::unchecked("ibc_transfer_adapter"));
}
