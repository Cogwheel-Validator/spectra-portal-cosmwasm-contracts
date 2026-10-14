use cosmwasm_std::{
    testing::{mock_dependencies_with_balances, mock_env},
    Addr, BankMsg, Coin, CosmosMsg,
};
use skip::{ibc::IbcLifecycleComplete, sudo::OsmosisSudoMsg as SudoMsg};
use skip_go_ibc_adapter_ibc_hooks::{error::ContractResult, state::ACK_ID_TO_RECOVER_ADDRESS};

#[test]
fn test_timeout_refunds_entire_balance_including_unrelated_dust() -> ContractResult<()> {
    // The contract holds the failed transfer coin (100 uosmo) PLUS unrelated
    // dust (7 uatom) that did not belong to this transfer.
    let mut deps = mock_dependencies_with_balances(&[(
        "ibc_transfer_adapter",
        &[Coin::new(100, "uosmo"), Coin::new(7, "uatom")],
    )]);

    let mut env = mock_env();
    env.contract.address = Addr::unchecked("ibc_transfer_adapter");

    ACK_ID_TO_RECOVER_ADDRESS.save(
        deps.as_mut().storage,
        ("channel_id", 1),
        &"recover_address".to_string(),
    )?;

    let res = skip_go_ibc_adapter_ibc_hooks::contract::sudo(
        deps.as_mut(),
        env,
        SudoMsg::IbcLifecycleComplete(IbcLifecycleComplete::IbcTimeout {
            channel: "channel_id".to_string(),
            sequence: 1,
        }),
    )
    .unwrap();

    // A single bank send carries BOTH coins to the recover address, even though
    // only 100 uosmo was part of this transfer.
    assert_eq!(res.messages.len(), 1);
    let CosmosMsg::Bank(BankMsg::Send { to_address, amount }) = &res.messages[0].msg else {
        panic!("expected a bank send");
    };
    assert_eq!(to_address, "recover_address");
    // Both the failed-transfer coin AND the unrelated dust are swept to the one
    // recover address (order is querier-dependent, so assert membership).
    assert_eq!(amount.len(), 2);
    assert!(amount.contains(&Coin::new(100, "uosmo")));
    assert!(amount.contains(&Coin::new(7, "uatom")));

    Ok(())
}
