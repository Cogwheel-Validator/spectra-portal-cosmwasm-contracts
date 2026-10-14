use astroport::asset::{Asset as AstroportAsset, AssetInfo};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    from_json,
    testing::{mock_dependencies, mock_env},
    to_json_binary, Addr, Coin, ContractResult as SystemContractResult, QuerierResult,
    SystemResult, Uint128, WasmQuery,
};
use cw20::Cw20Coin;
use skip::{
    asset::Asset,
    swap::{QueryMsg, SwapOperation},
};
use skip_go_swap_adapter_hallswap::{
    error::ContractError, state::HALLSWAP_CONTRACT_ADDRESS,
};

// Local mirror of the private HallswapQuerySimulationResult response type.
#[cw_serde]
struct MirrorSimulationResult {
    return_asset: AstroportAsset,
    fee_asset: Option<AstroportAsset>,
}

fn setup_with_simulation(return_asset: AstroportAsset) -> cosmwasm_std::OwnedDeps<
    cosmwasm_std::testing::MockStorage,
    cosmwasm_std::testing::MockApi,
    cosmwasm_std::testing::MockQuerier,
> {
    let mut deps = mock_dependencies();

    let result = MirrorSimulationResult {
        return_asset,
        fee_asset: None,
    };
    let handler = move |query: &WasmQuery| -> QuerierResult {
        match query {
            WasmQuery::Smart { contract_addr, .. } if contract_addr == "hallswap_contract" => {
                SystemResult::Ok(SystemContractResult::Ok(to_json_binary(&result).unwrap()))
            }
            _ => panic!("unsupported query: {:?}", query),
        }
    };
    deps.querier.update_wasm(handler);
    HALLSWAP_CONTRACT_ADDRESS
        .save(deps.as_mut().storage, &Addr::unchecked("hallswap_contract"))
        .unwrap();
    deps
}

#[test]
fn test_simulate_swap_exact_asset_in_native_out() {
    let mut deps = setup_with_simulation(AstroportAsset {
        info: AssetInfo::NativeToken {
            denom: "ua".to_string(),
        },
        amount: Uint128::new(90),
    });

    let res: Asset = from_json(
        skip_go_swap_adapter_hallswap::contract::query(
            deps.as_mut().as_ref(),
            mock_env(),
            QueryMsg::SimulateSwapExactAssetIn {
                asset_in: Asset::Native(Coin::new(100, "os")),
                swap_operations: vec![SwapOperation {
                    pool: "pool1".to_string(),
                    denom_in: "os".to_string(),
                    denom_out: "ua".to_string(),
                    interface: None,
                }],
            },
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(res, Asset::Native(Coin::new(90, "ua")));
}

#[test]
fn test_simulate_swap_exact_asset_in_cw20_out() {
    let mut deps = setup_with_simulation(AstroportAsset {
        info: AssetInfo::Token {
            contract_addr: Addr::unchecked("cw20out"),
        },
        amount: Uint128::new(77),
    });

    let res: Asset = from_json(
        skip_go_swap_adapter_hallswap::contract::query(
            deps.as_mut().as_ref(),
            mock_env(),
            QueryMsg::SimulateSwapExactAssetIn {
                asset_in: Asset::Native(Coin::new(100, "os")),
                swap_operations: vec![SwapOperation {
                    pool: "pool1".to_string(),
                    denom_in: "os".to_string(),
                    denom_out: "cw20out".to_string(),
                    interface: None,
                }],
            },
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(
        res,
        Asset::Cw20(Cw20Coin {
            address: "cw20out".to_string(),
            amount: Uint128::new(77),
        })
    );
}

#[test]
fn test_simulate_swap_empty_operations_errors() {
    let mut deps = setup_with_simulation(AstroportAsset {
        info: AssetInfo::NativeToken {
            denom: "ua".to_string(),
        },
        amount: Uint128::new(90),
    });

    let err = skip_go_swap_adapter_hallswap::contract::query(
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
fn test_simulate_swap_denom_mismatch_errors() {
    let mut deps = setup_with_simulation(AstroportAsset {
        info: AssetInfo::NativeToken {
            denom: "ua".to_string(),
        },
        amount: Uint128::new(90),
    });

    // asset_in denom ("wrong") does not match the first operation's denom_in ("os").
    let err = skip_go_swap_adapter_hallswap::contract::query(
        deps.as_mut().as_ref(),
        mock_env(),
        QueryMsg::SimulateSwapExactAssetIn {
            asset_in: Asset::Native(Coin::new(100, "wrong")),
            swap_operations: vec![SwapOperation {
                pool: "pool1".to_string(),
                denom_in: "os".to_string(),
                denom_out: "ua".to_string(),
                interface: None,
            }],
        },
    )
    .unwrap_err();

    assert!(matches!(err, ContractError::CoinInDenomMismatch));
}
