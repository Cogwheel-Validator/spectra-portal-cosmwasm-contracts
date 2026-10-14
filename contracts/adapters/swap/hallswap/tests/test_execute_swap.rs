use astroport::asset::AssetInfo;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    from_json,
    testing::{mock_dependencies, mock_env, mock_info},
    Addr, Binary, Coin, CosmosMsg, Uint128, WasmMsg,
};
use skip::swap::{ExecuteMsg, SwapOperation};
use skip_go_swap_adapter_hallswap::{
    error::{ContractError, ContractResult},
    state::{ENTRY_POINT_CONTRACT_ADDRESS, HALLSWAP_CONTRACT_ADDRESS},
};
use test_case::test_case;

// Local mirror of the (private) hallswap message types so we can decode and
// assert on the message the adapter emits without touching contract source.
#[cw_serde]
enum MirrorHallswapExecuteMsg {
    ExecuteRoutesV2 {
        routes: Vec<MirrorRouteInfo>,
        minimum_receive: Uint128,
        to: Option<Addr>,
    },
}

#[cw_serde]
struct MirrorRouteInfo {
    route: Vec<MirrorSwapOperation>,
    offer_amount: Uint128,
}

#[cw_serde]
struct MirrorSwapOperation {
    contract_addr: Addr,
    offer_asset: AssetInfo,
    return_asset: AssetInfo,
    interface: Option<MirrorInterface>,
}

#[cw_serde]
enum MirrorInterface {
    Binary(Binary),
}

/*
Test Cases:

Expect Success
    - One Swap Operation (wraps into a single ExecuteRoutesV2 hallswap message)
    - Multiple Swap Operations
    - Zero Swap Operations (NOTE: unlike the astroport adapter, hallswap does NOT
      reject an empty operations vector; it forwards an empty route.

Expect Error
    - Unauthorized Caller (only the stored entry point contract may call Swap)
    - No Coin Sent
    - More Than One Coin Sent
 */

struct Params {
    caller: String,
    info_funds: Vec<Coin>,
    swap_operations: Vec<SwapOperation>,
    expected_routes: Option<Vec<MirrorRouteInfo>>,
    expected_error: Option<ContractError>,
}

#[test_case(
    Params {
        caller: "entry_point".to_string(),
        info_funds: vec![Coin::new(100, "os")],
        swap_operations: vec![SwapOperation {
            pool: "pool1".to_string(),
            denom_in: "os".to_string(),
            denom_out: "ua".to_string(),
            interface: None,
        }],
        expected_routes: Some(vec![MirrorRouteInfo {
            route: vec![MirrorSwapOperation {
                contract_addr: Addr::unchecked("pool1"),
                offer_asset: AssetInfo::NativeToken { denom: "os".to_string() },
                return_asset: AssetInfo::NativeToken { denom: "ua".to_string() },
                interface: None,
            }],
            offer_amount: Uint128::new(100),
        }]),
        expected_error: None,
    };
    "One Swap Operation")]
#[test_case(
    Params {
        caller: "entry_point".to_string(),
        info_funds: vec![Coin::new(100, "os")],
        swap_operations: vec![
            SwapOperation {
                pool: "pool1".to_string(),
                denom_in: "os".to_string(),
                denom_out: "ua".to_string(),
                interface: None,
            },
            SwapOperation {
                pool: "pool2".to_string(),
                denom_in: "ua".to_string(),
                denom_out: "un".to_string(),
                interface: None,
            },
        ],
        expected_routes: Some(vec![MirrorRouteInfo {
            route: vec![
                MirrorSwapOperation {
                    contract_addr: Addr::unchecked("pool1"),
                    offer_asset: AssetInfo::NativeToken { denom: "os".to_string() },
                    return_asset: AssetInfo::NativeToken { denom: "ua".to_string() },
                    interface: None,
                },
                MirrorSwapOperation {
                    contract_addr: Addr::unchecked("pool2"),
                    offer_asset: AssetInfo::NativeToken { denom: "ua".to_string() },
                    return_asset: AssetInfo::NativeToken { denom: "un".to_string() },
                    interface: None,
                },
            ],
            offer_amount: Uint128::new(100),
        }]),
        expected_error: None,
    };
    "Multiple Swap Operations")]
#[test_case(
    Params {
        caller: "entry_point".to_string(),
        info_funds: vec![Coin::new(100, "os")],
        swap_operations: vec![],
        expected_routes: Some(vec![MirrorRouteInfo {
            route: vec![],
            offer_amount: Uint128::new(100),
        }]),
        expected_error: None,
    };
    "Zero Swap Operations Forwarded (no rejection)")]
#[test_case(
    Params {
        caller: "random".to_string(),
        info_funds: vec![Coin::new(100, "os")],
        swap_operations: vec![],
        expected_routes: None,
        expected_error: Some(ContractError::Unauthorized),
    };
    "Unauthorized Caller - Expect Error")]
#[test_case(
    Params {
        caller: "entry_point".to_string(),
        info_funds: vec![],
        swap_operations: vec![],
        expected_routes: None,
        expected_error: Some(ContractError::Payment(cw_utils::PaymentError::NoFunds {})),
    };
    "No Coin Sent - Expect Error")]
#[test_case(
    Params {
        caller: "entry_point".to_string(),
        info_funds: vec![Coin::new(100, "un"), Coin::new(100, "os")],
        swap_operations: vec![],
        expected_routes: None,
        expected_error: Some(ContractError::Payment(cw_utils::PaymentError::MultipleDenoms {})),
    };
    "More Than One Coin Sent - Expect Error")]
fn test_execute_swap(params: Params) -> ContractResult<()> {
    let mut deps = mock_dependencies();

    let mut env = mock_env();
    env.contract.address = Addr::unchecked("swap_contract_address");

    let info = mock_info(&params.caller, &params.info_funds);

    ENTRY_POINT_CONTRACT_ADDRESS.save(deps.as_mut().storage, &Addr::unchecked("entry_point"))?;
    HALLSWAP_CONTRACT_ADDRESS.save(deps.as_mut().storage, &Addr::unchecked("hallswap_contract"))?;

    let res = skip_go_swap_adapter_hallswap::contract::execute(
        deps.as_mut(),
        env,
        info,
        ExecuteMsg::Swap {
            operations: params.swap_operations.clone(),
        },
    );

    match res {
        Ok(res) => {
            assert!(
                params.expected_error.is_none(),
                "expected error {:?} but succeeded",
                params.expected_error
            );

            // Exactly one message: the ExecuteRoutesV2 call to the hallswap contract.
            assert_eq!(res.messages.len(), 1);

            let CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr,
                msg,
                funds,
            }) = res.messages[0].msg.clone()
            else {
                panic!("expected a WasmMsg::Execute");
            };

            assert_eq!(contract_addr, "hallswap_contract");
            // The native funds are forwarded verbatim to the hallswap contract.
            assert_eq!(funds, vec![Coin::new(100, "os")]);

            let decoded: MirrorHallswapExecuteMsg = from_json(&msg).unwrap();
            let MirrorHallswapExecuteMsg::ExecuteRoutesV2 {
                routes,
                minimum_receive,
                to,
            } = decoded;

            assert_eq!(routes, params.expected_routes.unwrap());
            // NOTE: the adapter hard-codes minimum_receive to zero and relies on the
            // entry point contract to enforce min_asset.
            assert_eq!(minimum_receive, Uint128::zero());
            assert_eq!(to, Some(Addr::unchecked("entry_point")));
        }
        Err(err) => {
            assert_eq!(err, params.expected_error.unwrap());
        }
    }

    Ok(())
}
