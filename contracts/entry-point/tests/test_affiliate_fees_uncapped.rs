use cosmwasm_std::{
    testing::{mock_dependencies_with_balances, mock_env, mock_info},
    Addr, BankMsg, Coin, CosmosMsg, Timestamp, Uint128,
};
use skip::{
    asset::Asset,
    entry_point::{Affiliate, ExecuteMsg},
    swap::{Swap, SwapExactAssetIn, SwapOperation},
};
use skip_go_entry_point::state::SWAP_VENUE_MAP;

// `execute_user_swap` calculates each affiliate fee as
// `min_asset.amount * basis_points_fee / 10000` and dispatches a bank send for
// each, but it NEVER checks that the sum of all affiliates' basis points is
// <= 10000 (100%). As a result a caller can specify affiliates whose fees add
// up to more than the entire minimum output. The affiliate bank sends are
// emitted after the swap message, so on execution they would attempt to pay out
// more than `min_asset`.
//
// These tests pin the *current* (uncapped) behavior: the contract accepts the
// oversized affiliate configuration and emits the corresponding sends WITHOUT
// erroring. If a cap is later added, these tests will fail and must be updated,
// making the fix an explicit, reviewable change.

fn setup() -> cosmwasm_std::OwnedDeps<
    cosmwasm_std::testing::MockStorage,
    cosmwasm_std::testing::MockApi,
    cosmwasm_std::testing::MockQuerier,
> {
    let mut deps = mock_dependencies_with_balances(&[(
        "entry_point",
        &[Coin::new(1_000_000, "os"), Coin::new(1_000_000, "un")],
    )]);
    SWAP_VENUE_MAP
        .save(
            deps.as_mut().storage,
            "swap_venue_name",
            &Addr::unchecked("swap_venue_adapter"),
        )
        .unwrap();
    deps
}

fn call_user_swap(
    deps: cosmwasm_std::DepsMut,
    affiliates: Vec<Affiliate>,
) -> Result<cosmwasm_std::Response, skip_go_entry_point::error::ContractError> {
    let mut env = mock_env();
    env.contract.address = Addr::unchecked("entry_point");
    env.block.time = Timestamp::from_nanos(100);

    skip_go_entry_point::contract::execute(
        deps,
        env,
        mock_info("entry_point", &[]),
        ExecuteMsg::UserSwap {
            swap: Swap::SwapExactAssetIn(SwapExactAssetIn {
                swap_venue_name: "swap_venue_name".to_string(),
                operations: vec![SwapOperation {
                    pool: "pool".to_string(),
                    denom_in: "un".to_string(),
                    denom_out: "os".to_string(),
                    interface: None,
                }],
            }),
            remaining_asset: Asset::Native(Coin::new(1_000_000, "un")),
            min_asset: Asset::Native(Coin::new(1_000_000, "os")),
            affiliates,
        },
    )
}

// Sum of affiliate fees exceeds the min output. Fees: 6000bps + 6000bps = 120%.
#[test]
fn test_two_affiliates_exceeding_100_percent_is_accepted() {
    let mut deps = setup();

    let res = call_user_swap(
        deps.as_mut(),
        vec![
            Affiliate {
                address: "affiliate_1".to_string(),
                basis_points_fee: Uint128::new(6000),
            },
            Affiliate {
                address: "affiliate_2".to_string(),
                basis_points_fee: Uint128::new(6000),
            },
        ],
    )
    .expect("current behavior: no cap enforced, so this succeeds");

    // 1 swap message + 2 affiliate bank sends.
    assert_eq!(res.messages.len(), 3);

    // Total affiliate payout demanded = 1_200_000 os, which is GREATER than the
    // 1_000_000 os min output the user asked to receive.
    let total_affiliate: u128 = res
        .messages
        .iter()
        .filter_map(|m| match &m.msg {
            CosmosMsg::Bank(BankMsg::Send { amount, .. }) => Some(amount[0].amount.u128()),
            _ => None,
        })
        .sum();
    assert_eq!(total_affiliate, 1_200_000);
    assert!(total_affiliate > 1_000_000);
}

// A single affiliate above 100% (15000bps = 150%) is likewise accepted.
#[test]
fn test_single_affiliate_over_100_percent_is_accepted() {
    let mut deps = setup();

    let res = call_user_swap(
        deps.as_mut(),
        vec![Affiliate {
            address: "affiliate".to_string(),
            basis_points_fee: Uint128::new(15000),
        }],
    )
    .expect("current behavior: no cap enforced");

    let affiliate_amount = res
        .messages
        .iter()
        .find_map(|m| match &m.msg {
            CosmosMsg::Bank(BankMsg::Send { amount, .. }) => Some(amount[0].amount.u128()),
            _ => None,
        })
        .unwrap();

    assert_eq!(affiliate_amount, 1_500_000);
    assert!(affiliate_amount > 1_000_000);
}
