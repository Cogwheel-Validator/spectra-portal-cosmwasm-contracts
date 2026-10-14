use cosmwasm_std::{
    from_json,
    testing::{mock_dependencies, mock_env},
    to_json_binary, Addr, Coin, ContractResult as SystemContractResult, QuerierResult,
    SystemResult, Uint128, WasmQuery,
};
use skip::{
    asset::Asset,
    swap::{QueryMsg, SwapOperation},
};
use skip_go_swap_adapter_injective::{
    contract::{query, InputQuantityResponse, OutputQuantityResponse},
    error::ContractError,
    state::INJECTIVE_SWAP_CONTRACT_ADDRESS,
};

fn deps_with_injective_quantity(
    output: Option<u128>,
    input: Option<u128>,
) -> cosmwasm_std::OwnedDeps<
    cosmwasm_std::testing::MockStorage,
    cosmwasm_std::testing::MockApi,
    cosmwasm_std::testing::MockQuerier,
> {
    let mut deps = mock_dependencies();
    let handler = move |query: &WasmQuery| -> QuerierResult {
        match query {
            WasmQuery::Smart { contract_addr, msg } if contract_addr == "injective_swap" => {
                // Distinguish the two query variants by inspecting the raw JSON.
                let raw = String::from_utf8_lossy(msg.as_slice());
                let is_output = raw.contains("get_output_quantity");
                if is_output {
                    SystemResult::Ok(SystemContractResult::Ok(
                        to_json_binary(&OutputQuantityResponse {
                            quantity: Uint128::new(output.unwrap()),
                        })
                        .unwrap(),
                    ))
                } else {
                    SystemResult::Ok(SystemContractResult::Ok(
                        to_json_binary(&InputQuantityResponse {
                            quantity: Uint128::new(input.unwrap()),
                        })
                        .unwrap(),
                    ))
                }
            }
            _ => panic!("unsupported query: {:?}", query),
        }
    };
    deps.querier.update_wasm(handler);
    INJECTIVE_SWAP_CONTRACT_ADDRESS
        .save(deps.as_mut().storage, &Addr::unchecked("injective_swap"))
        .unwrap();
    deps
}

#[test]
fn test_simulate_swap_exact_asset_in() {
    let mut deps = deps_with_injective_quantity(Some(90), None);

    let out: Asset = from_json(
        query(
            deps.as_mut().as_ref(),
            mock_env(),
            QueryMsg::SimulateSwapExactAssetIn {
                asset_in: Asset::Native(Coin::new(100, "os")),
                swap_operations: vec![SwapOperation {
                    pool: "1".to_string(),
                    denom_in: "os".to_string(),
                    denom_out: "ua".to_string(),
                    interface: None,
                }],
            },
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(out, Asset::Native(Coin::new(90, "ua")));
}

#[test]
fn test_simulate_swap_exact_asset_out() {
    let mut deps = deps_with_injective_quantity(None, Some(120));

    let asset_in: Asset = from_json(
        query(
            deps.as_mut().as_ref(),
            mock_env(),
            QueryMsg::SimulateSwapExactAssetOut {
                asset_out: Asset::Native(Coin::new(100, "ua")),
                swap_operations: vec![SwapOperation {
                    pool: "1".to_string(),
                    denom_in: "os".to_string(),
                    denom_out: "ua".to_string(),
                    interface: None,
                }],
            },
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(asset_in, Asset::Native(Coin::new(120, "os")));
}

#[test]
fn test_simulate_swap_empty_operations_errors() {
    let mut deps = deps_with_injective_quantity(Some(1), None);

    let err = query(
        deps.as_mut().as_ref(),
        mock_env(),
        QueryMsg::SimulateSwapExactAssetIn {
            asset_in: Asset::Native(Coin::new(100, "os")),
            swap_operations: vec![],
        },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::SwapOperationsEmpty));
}

#[test]
fn test_simulate_swap_in_denom_mismatch_errors() {
    let mut deps = deps_with_injective_quantity(Some(1), None);

    let err = query(
        deps.as_mut().as_ref(),
        mock_env(),
        QueryMsg::SimulateSwapExactAssetIn {
            asset_in: Asset::Native(Coin::new(100, "wrong")),
            swap_operations: vec![SwapOperation {
                pool: "1".to_string(),
                denom_in: "os".to_string(),
                denom_out: "ua".to_string(),
                interface: None,
            }],
        },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::CoinInDenomMismatch));
}
