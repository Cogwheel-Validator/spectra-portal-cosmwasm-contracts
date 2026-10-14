use astroport::asset::AssetInfo;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    from_json,
    testing::{mock_dependencies, mock_env, mock_info},
    to_json_binary, Addr, Binary, ContractResult as SystemContractResult, CosmosMsg, QuerierResult,
    SystemResult, Uint128, WasmMsg, WasmQuery,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20ReceiveMsg};
use skip::swap::{ExecuteMsg, SwapOperation};
use skip_go_swap_adapter_hallswap::{
    error::ContractResult,
    state::{ENTRY_POINT_CONTRACT_ADDRESS, HALLSWAP_CONTRACT_ADDRESS},
};

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

// A cw20 send goes through Receive: the adapter wraps the ExecuteRoutesV2 call
// inside a Cw20 Send to the token contract, routing the returned funds back to
// the entry point.
#[test]
fn test_receive_cw20_wraps_in_send() -> ContractResult<()> {
    let mut deps = mock_dependencies();

    // The token contract answers the balance query performed during validation.
    let wasm_handler = |query: &WasmQuery| -> QuerierResult {
        match query {
            WasmQuery::Smart { contract_addr, .. } if contract_addr == "cw20token" => {
                SystemResult::Ok(SystemContractResult::Ok(
                    to_json_binary(&BalanceResponse {
                        balance: Uint128::new(100),
                    })
                    .unwrap(),
                ))
            }
            _ => panic!("unsupported query: {:?}", query),
        }
    };
    deps.querier.update_wasm(wasm_handler);

    let mut env = mock_env();
    env.contract.address = Addr::unchecked("swap_contract_address");

    // The message arrives from the cw20 token contract (info.sender), with no native funds.
    let info = mock_info("cw20token", &[]);

    ENTRY_POINT_CONTRACT_ADDRESS.save(deps.as_mut().storage, &Addr::unchecked("entry_point"))?;
    HALLSWAP_CONTRACT_ADDRESS.save(deps.as_mut().storage, &Addr::unchecked("hallswap_contract"))?;

    let res = skip_go_swap_adapter_hallswap::contract::execute(
        deps.as_mut(),
        env,
        info,
        ExecuteMsg::Receive(Cw20ReceiveMsg {
            // The originating swapper is the entry point contract.
            sender: "entry_point".to_string(),
            amount: Uint128::new(100),
            msg: to_json_binary(&ExecuteMsg::Swap {
                operations: vec![SwapOperation {
                    pool: "pool1".to_string(),
                    denom_in: "cw20token".to_string(),
                    denom_out: "ua".to_string(),
                    interface: None,
                }],
            })?,
        }),
    )?;

    assert_eq!(res.messages.len(), 1);
    let CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr,
        msg,
        funds,
    }) = res.messages[0].msg.clone()
    else {
        panic!("expected WasmMsg::Execute");
    };

    // Outer message targets the token contract with a Send.
    assert_eq!(contract_addr, "cw20token");
    assert!(funds.is_empty());

    let Cw20ExecuteMsg::Send {
        contract,
        amount,
        msg: inner,
    } = from_json(&msg).unwrap()
    else {
        panic!("expected Cw20ExecuteMsg::Send");
    };
    assert_eq!(contract, "hallswap_contract");
    assert_eq!(amount, Uint128::new(100));

    let MirrorHallswapExecuteMsg::ExecuteRoutesV2 {
        routes,
        minimum_receive,
        to,
    } = from_json(&inner).unwrap();

    assert_eq!(
        routes,
        vec![MirrorRouteInfo {
            route: vec![MirrorSwapOperation {
                contract_addr: Addr::unchecked("pool1"),
                // A cw20 denom_in is classified as an astroport Token.
                offer_asset: AssetInfo::Token {
                    contract_addr: Addr::unchecked("cw20token")
                },
                return_asset: AssetInfo::NativeToken {
                    denom: "ua".to_string()
                },
                interface: None,
            }],
            offer_amount: Uint128::new(100),
        }]
    );
    assert_eq!(minimum_receive, Uint128::zero());
    assert_eq!(to, Some(Addr::unchecked("entry_point")));

    Ok(())
}
