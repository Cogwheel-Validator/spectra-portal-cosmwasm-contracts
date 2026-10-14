import asyncio
import json
import sys
import tomllib
from datetime import UTC, datetime
from pathlib import Path
from typing import NotRequired, TypedDict, cast

import tomlkit
from google.protobuf.message import Message
from pyinjective.async_client_v2 import AsyncClient
from pyinjective.composer_v2 import Composer
from pyinjective.constant import GAS_FEE_BUFFER_AMOUNT, GAS_PRICE
from pyinjective.core.network import Network
from pyinjective.proto.cosmwasm.wasm.v1 import tx_pb2 as wasm_tx_pb
from pyinjective.transaction import Transaction
from pyinjective.wallet import PrivateKey, PublicKey


class SwapVenue(TypedDict):
    name: str
    code_id: int
    hallswap_contract_address: NotRequired[str]


class Config(TypedDict):
    MNEMONIC: str
    COMMIT_HASH: str
    MAINNET_CHAIN_ID: str
    ENTRY_POINT_CODE_ID: int
    IBC_TRANSFER_CODE_ID: int
    SALT: str
    ENTRY_POINT_PRE_GENERATED_ADDRESS: str
    swap_venues: list[SwapVenue]


class GasInfo(TypedDict):
    gasUsed: str


class SimulateResponse(TypedDict):
    gasInfo: GasInfo


class BroadcastTxResponse(TypedDict):
    txhash: str


class BroadcastResponse(TypedDict):
    txResponse: BroadcastTxResponse


class EventAttribute(TypedDict):
    key: str
    value: str


class Event(TypedDict):
    type: str
    attributes: list[EventAttribute]


class TxResponse(TypedDict):
    events: list[Event]


class GetTxResponse(TypedDict):
    txResponse: TxResponse


class DeployError(Exception):
    pass


if len(sys.argv) != 3:
    sys.exit("Usage: python deploy_injective.py injective mainnet")

CHAIN = sys.argv[1]
NETWORK = sys.argv[2]

if CHAIN != "injective":
    sys.exit("Must specify injective chain for 1st cli arg.")
if NETWORK == "testnet":
    sys.exit("Testnet not supported.")
if NETWORK == "localnet":
    # TODO: Implement localnet support for CI and testing
    sys.exit("Localnet not supported yes...")
if NETWORK != "mainnet":
    sys.exit("Must specify either 'mainnet' or 'testnet' for 2nd cli arg.")

with Path(f"configs/{CHAIN}.toml").open("rb") as config_file:
    # tomllib returns an untyped dict; the TypedDict documents the expected config shape
    config = cast(Config, cast(object, tomllib.load(config_file)))

DEPLOYED_CONTRACTS_PATH = Path("../deployed-contracts") / CHAIN / f"{NETWORK}.toml"
DEPLOYED_CONTRACTS_PATH.parent.mkdir(parents=True, exist_ok=True)

network = Network.mainnet()
CHAIN_ID = config["MAINNET_CHAIN_ID"]
SWAP_VENUES = config["swap_venues"]

# SALT
SALT = config["SALT"].encode("utf-8")

# Pregenerated Contract Addresses
ENTRY_POINT_PRE_GENERATED_ADDRESS = config["ENTRY_POINT_PRE_GENERATED_ADDRESS"]

# MNEMONIC
MNEMONIC = config["MNEMONIC"]

# Code IDs
ENTRY_POINT_CODE_ID = config["ENTRY_POINT_CODE_ID"]
IBC_TRANSFER_CODE_ID = config["IBC_TRANSFER_CODE_ID"]

DEPLOYED_CONTRACTS_INFO: dict[str, dict[str, str | int]] = {}


async def main() -> None:
    # Initialize deployed contracts info
    init_deployed_contracts_info()

    client = AsyncClient(network)
    composer = await client.composer()
    await client.sync_timeout_height()

    # load account
    priv_key = PrivateKey.from_mnemonic(MNEMONIC)
    pub_key = priv_key.to_public_key()
    address = pub_key.to_address().to_acc_bech32()
    _ = await client.fetch_account(address)

    balances = await client.fetch_bank_balances(address)
    print("Address: ", address, " with account balance: ", balances)

    ibc_transfer_adapter_contract_address = await instantiate_contract(
        client,
        composer,
        priv_key,
        pub_key,
        address,
        code_id=IBC_TRANSFER_CODE_ID,
        args={"entry_point_contract_address": ENTRY_POINT_PRE_GENERATED_ADDRESS},
        label="Skip Swap IBC Transfer Adapter",
        name="ibc_transfer_adapter",
    )

    swap_venues: list[dict[str, str]] = []

    for venue in SWAP_VENUES:
        args = {"entry_point_contract_address": ENTRY_POINT_PRE_GENERATED_ADDRESS}
        if "hallswap_contract_address" in venue:
            args["hallswap_contract_address"] = venue["hallswap_contract_address"]
        swap_adapter_contract_address = await instantiate_contract(
            client,
            composer,
            priv_key,
            pub_key,
            address,
            code_id=venue["code_id"],
            args=args,
            label=f"Skip Swap Swap Adapter {venue['name']}",
            name=f"swap_adapter_{venue['name']}",
        )
        swap_venues.append(
            {
                "name": venue["name"],
                "adapter_contract_address": swap_adapter_contract_address,
            }
        )

    _ = await instantiate2_contract(
        client,
        composer,
        priv_key,
        pub_key,
        address,
        code_id=ENTRY_POINT_CODE_ID,
        args={
            "swap_venues": swap_venues,
            "ibc_transfer_contract_address": ibc_transfer_adapter_contract_address,
        },
        label="Skip Swap Entry Point",
        name="entry_point",
    )


async def instantiate_contract(
    client: AsyncClient,
    composer: Composer,
    priv_key: PrivateKey,
    pub_key: PublicKey,
    address: str,
    code_id: int,
    args: dict[str, str],
    label: str,
    name: str,
) -> str:
    msg = wasm_tx_pb.MsgInstantiateContract(
        sender=address,
        code_id=code_id,
        label=label,
        msg=json.dumps(args).encode("utf-8"),
    )
    return await broadcast_tx(msg, client, composer, priv_key, pub_key, code_id, name)


async def instantiate2_contract(
    client: AsyncClient,
    composer: Composer,
    priv_key: PrivateKey,
    pub_key: PublicKey,
    address: str,
    code_id: int,
    args: dict[str, str | list[dict[str, str]]],
    label: str,
    name: str,
) -> str:
    msg = wasm_tx_pb.MsgInstantiateContract2(
        sender=address,
        code_id=code_id,
        label=label,
        msg=json.dumps(args).encode("utf-8"),
        salt=SALT,
        fix_msg=False,
    )
    return await broadcast_tx(msg, client, composer, priv_key, pub_key, code_id, name)


async def broadcast_tx(
    msg: Message,
    client: AsyncClient,
    composer: Composer,
    priv_key: PrivateKey,
    pub_key: PublicKey,
    code_id: int,
    name: str,
) -> str:
    # build sim tx
    tx = (
        Transaction()
        .with_messages(msg)
        .with_sequence(client.get_sequence())
        .with_account_num(client.get_number())
        .with_chain_id(network.chain_id)
    )
    sim_sign_doc = tx.get_sign_doc(pub_key)
    sim_sig = priv_key.sign(sim_sign_doc.SerializeToString())
    sim_tx_raw_bytes = tx.get_tx_data(sim_sig, pub_key)

    # simulate tx
    sim_res = cast(SimulateResponse, cast(object, await client.simulate(sim_tx_raw_bytes)))
    print("sim_res: ", sim_res)

    # build tx
    gas_price = GAS_PRICE
    gas_limit = (
        int(sim_res["gasInfo"]["gasUsed"]) + GAS_FEE_BUFFER_AMOUNT + 100_000
    )  # add buffer for gas fee computation
    gas_fee = f"{(gas_price * gas_limit) / pow(10, 18):.18f}".rstrip("0")
    fee = [composer.coin(amount=gas_price * gas_limit, denom=network.fee_denom)]
    tx = (
        tx.with_gas(gas_limit)
        .with_fee(fee)
        .with_memo("")
        .with_timeout_height(client.timeout_height)
    )
    sign_doc = tx.get_sign_doc(pub_key)
    sig = priv_key.sign(sign_doc.SerializeToString())
    tx_raw_bytes = tx.get_tx_data(sig, pub_key)

    print("tx: ", tx)
    print("gas price: ", gas_price)
    print("gas limit: ", gas_limit)
    print("gas fee: ", gas_fee)
    print("fee: ", fee)
    print("Broadcasting tx...")

    # broadcast tx: send_tx_async_mode, send_tx_sync_mode, send_tx_block_mode
    res = cast(BroadcastResponse, cast(object, await client.broadcast_tx_sync_mode(tx_raw_bytes)))
    print(res)
    print(f"gas wanted: {gas_limit}")
    print(f"gas fee: {gas_fee} INJ")

    print("Sleeping for 10 seconds...")
    await asyncio.sleep(10)

    tx_hash = res["txResponse"]["txhash"]
    print("tx hash: ", tx_hash)
    tx_logs = cast(GetTxResponse, cast(object, await client.fetch_tx(hash=tx_hash)))
    print(tx_logs)
    contract_address = get_contract_address(tx_logs)

    print(f"Skip Swap {name} Contract Address:", contract_address)
    DEPLOYED_CONTRACTS_INFO["code-ids"][f"{name}_contract_code_id"] = code_id
    DEPLOYED_CONTRACTS_INFO["contract-addresses"][f"{name}_contract_address"] = contract_address
    DEPLOYED_CONTRACTS_INFO["tx-hashes"][f"instantiate_{name}_tx_hash"] = tx_hash
    save_deployed_contracts_info()

    return contract_address


def get_contract_address(tx_logs: GetTxResponse) -> str:
    for event in tx_logs["txResponse"]["events"]:
        if event["type"] == "instantiate":
            for attr in event["attributes"]:
                if attr["key"] == "_contract_address":
                    return attr["value"]
    raise DeployError("Contract address not found in tx response")


def init_deployed_contracts_info():
    DEPLOYED_CONTRACTS_INFO["info"] = {
        "chain_id": CHAIN_ID,
        "network": NETWORK,
        "deploy_date": datetime.now(UTC).strftime("%d/%m/%Y %H:%M:%S"),
        "commit_hash": config["COMMIT_HASH"],
        "salt": config["SALT"],
    }
    DEPLOYED_CONTRACTS_INFO["checksums"] = {}
    DEPLOYED_CONTRACTS_INFO["code-ids"] = {}
    DEPLOYED_CONTRACTS_INFO["contract-addresses"] = {}
    DEPLOYED_CONTRACTS_INFO["tx-hashes"] = {}

    # Get checksums for deployed contracts info
    checksums = Path("../artifacts/checksums.txt").read_text().split()
    for i in range(0, len(checksums), 2):
        DEPLOYED_CONTRACTS_INFO["checksums"][checksums[i + 1]] = checksums[i]
    save_deployed_contracts_info()


def save_deployed_contracts_info():
    with DEPLOYED_CONTRACTS_PATH.open("w") as f:
        tomlkit.dump(DEPLOYED_CONTRACTS_INFO, f)


if __name__ == "__main__":
    asyncio.run(main())
