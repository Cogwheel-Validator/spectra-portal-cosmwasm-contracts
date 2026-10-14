import json
import sys
import time
import tomllib
from base64 import b64encode
from datetime import UTC, datetime
from hashlib import sha256
from pathlib import Path
from typing import NotRequired, TypedDict, cast

import httpx
import tomlkit
from cosmpy.aerial.client import LedgerClient
from cosmpy.aerial.config import NetworkConfig
from cosmpy.aerial.tx import SigningCfg, Transaction, TxFee
from cosmpy.aerial.wallet import LocalWallet
from cosmpy.protos.cosmos.authz.v1beta1.tx_pb2 import MsgExec
from cosmpy.protos.cosmwasm.wasm.v1.tx_pb2 import (
    MsgInstantiateContract,
    MsgInstantiateContract2,
    MsgStoreCode,
)
from google.protobuf.any_pb2 import Any as ProtoAny
from google.protobuf.message import Message


class SwapVenue(TypedDict):
    name: str
    swap_adapter_path: str


class Config(TypedDict):
    MNEMONIC: str
    COMMIT_HASH: str
    MAINNET_REST_URL: str
    MAINNET_RPC_URL: str
    MAINNET_CHAIN_ID: str
    TESTNET_REST_URL: str
    TESTNET_RPC_URL: str
    TESTNET_CHAIN_ID: str
    ADDRESS_PREFIX: str
    DENOM: str
    GAS_PRICE: float
    ENTRY_POINT_CONTRACT_PATH: str
    IBC_TRANSFER_ADAPTER_PATH: str
    SALT: str
    ENTRY_POINT_PRE_GENERATED_ADDRESS: str
    PERMISSIONED_UPLOADER_ADDRESS: NotRequired[str]
    swap_venues: NotRequired[list[SwapVenue]]
    testnet_swap_venues: NotRequired[list[SwapVenue]]


class EventAttribute(TypedDict):
    key: str
    value: str


class Event(TypedDict):
    type: str
    attributes: list[EventAttribute]


class TxLog(TypedDict):
    events: list[Event]


class TxResponse(TypedDict):
    logs: NotRequired[list[TxLog]]
    events: list[Event]


class GetTxResponse(TypedDict):
    tx_response: TxResponse


class DeployError(Exception):
    pass


if len(sys.argv) != 3:
    sys.exit("Usage: python deploy.py osmosis <mainnet|testnet>")

CHAIN = sys.argv[1]
NETWORK = sys.argv[2]

if CHAIN == "injective":
    sys.exit("Injective is not supported in deploy.py. Use deploy_injective.py instead.")
if CHAIN != "osmosis":
    sys.exit(f"Unsupported chain {CHAIN}; deploy.py only supports osmosis.")
if NETWORK not in ("mainnet", "testnet"):
    sys.exit("Must specify either 'mainnet' or 'testnet' for 2nd cli arg.")

with Path(f"configs/{CHAIN}.toml").open("rb") as config_file:
    # tomllib returns an untyped dict; the TypedDict documents the expected config shape
    config = cast(Config, cast(object, tomllib.load(config_file)))

DEPLOYED_CONTRACTS_PATH = Path("../deployed-contracts") / CHAIN / f"{NETWORK}.toml"
DEPLOYED_CONTRACTS_PATH.parent.mkdir(parents=True, exist_ok=True)

IS_MAINNET = NETWORK == "mainnet"
REST_URL = config["MAINNET_REST_URL"] if IS_MAINNET else config["TESTNET_REST_URL"]
RPC_URL = config["MAINNET_RPC_URL"] if IS_MAINNET else config["TESTNET_RPC_URL"]
CHAIN_ID = config["MAINNET_CHAIN_ID"] if IS_MAINNET else config["TESTNET_CHAIN_ID"]
SWAP_VENUES = config.get("swap_venues", []) if IS_MAINNET else config.get("testnet_swap_venues", [])
PERMISSIONED_UPLOADER_ADDRESS = config.get("PERMISSIONED_UPLOADER_ADDRESS") if IS_MAINNET else None

ADDRESS_PREFIX = config["ADDRESS_PREFIX"]
DENOM = config["DENOM"]
GAS_PRICE = config["GAS_PRICE"]

STORE_GAS_LIMIT = 15_000_000
INSTANTIATE_GAS_LIMIT = 600_000

# Contract Paths
ENTRY_POINT_CONTRACT_PATH = config["ENTRY_POINT_CONTRACT_PATH"]
IBC_TRANSFER_ADAPTER_PATH = config["IBC_TRANSFER_ADAPTER_PATH"]

# SALT
SALT = config["SALT"].encode("utf-8")

# Pregenerated Contract Addresses
ENTRY_POINT_PRE_GENERATED_ADDRESS = config["ENTRY_POINT_PRE_GENERATED_ADDRESS"]

MNEMONIC = config["MNEMONIC"]

DEPLOYED_CONTRACTS_INFO: dict[str, dict[str, str]] = {}


def main():
    # Create network config and client
    cfg = NetworkConfig(
        chain_id=CHAIN_ID,
        url=f"rest+{REST_URL}",
        fee_minimum_gas_price=0.01,
        fee_denomination=DENOM,
        staking_denomination=DENOM,
    )
    client = LedgerClient(cfg)

    # Create wallet from mnemonic
    wallet = create_wallet(client)

    # Initialize deployed contracts info
    init_deployed_contracts_info()

    # Get checksums for deployed contracts info
    checksums = Path("../artifacts/checksums.txt").read_text().split()

    # Store checksums for deployed contracts info
    for i in range(0, len(checksums), 2):
        DEPLOYED_CONTRACTS_INFO["checksums"][checksums[i + 1]] = checksums[i]
    save_deployed_contracts_info()

    # Store and instantiate IBC transfer adapter contract
    ibc_transfer_adapter_contract_code_id = store_contract(
        client, wallet, IBC_TRANSFER_ADAPTER_PATH, "ibc_transfer_adapter"
    )
    ibc_transfer_adapter_contract_address = instantiate_contract(
        client,
        wallet,
        ibc_transfer_adapter_contract_code_id,
        {"entry_point_contract_address": ENTRY_POINT_PRE_GENERATED_ADDRESS},
        "Skip Swap IBC Transfer Adapter",
        "ibc_transfer_adapter",
    )

    swap_venues: list[dict[str, str]] = []

    # Swap Contracts
    for venue in SWAP_VENUES:
        swap_adapter_contract_code_id = store_contract(
            client, wallet, venue["swap_adapter_path"], f"swap_adapter_{venue['name']}"
        )
        swap_adapter_contract_address = instantiate_contract(
            client,
            wallet,
            swap_adapter_contract_code_id,
            {"entry_point_contract_address": ENTRY_POINT_PRE_GENERATED_ADDRESS},
            f"Skip Swap Swap Adapter {venue['name']}",
            f"swap_adapter_{venue['name']}",
        )
        swap_venues.append(
            {
                "name": venue["name"],
                "adapter_contract_address": swap_adapter_contract_address,
            }
        )

    # Entry Point Contract
    entry_point_contract_code_id = store_contract(
        client, wallet, ENTRY_POINT_CONTRACT_PATH, "entry_point"
    )
    _ = instantiate2_contract(
        client=client,
        wallet=wallet,
        code_id=entry_point_contract_code_id,
        args={
            "swap_venues": swap_venues,
            "ibc_transfer_contract_address": ibc_transfer_adapter_contract_address,
        },
        label="Skip Swap Entry Point",
        name="entry_point",
    )


def create_tx(
    msg: Message,
    client: LedgerClient,
    wallet: LocalWallet,
    gas_limit: int,
) -> Transaction:
    time.sleep(5)
    tx = Transaction()
    _ = tx.add_message(msg)

    # Get account
    account = client.query_account(wallet.address())

    # Seal, Sign, and Complete Tx
    _ = tx.seal(
        signing_cfgs=[SigningCfg.direct(wallet.public_key(), account.sequence)],
        fee=TxFee(amount=f"{int(GAS_PRICE * gas_limit)}{DENOM}", gas_limit=gas_limit),
    )
    _ = tx.sign(wallet.signer(), client.network_config.chain_id, account.number)
    _ = tx.complete()

    return tx


def create_wallet(client: LedgerClient) -> LocalWallet:
    """Create a wallet from a mnemonic and return it"""
    wallet = LocalWallet.from_mnemonic(MNEMONIC, prefix=ADDRESS_PREFIX)
    balance = client.query_bank_balance(wallet.address(), DENOM)
    print("Wallet Address: ", wallet.address(), " with account balance: ", balance)
    return wallet


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
    save_deployed_contracts_info()


def save_deployed_contracts_info():
    with DEPLOYED_CONTRACTS_PATH.open("w") as f:
        tomlkit.dump(DEPLOYED_CONTRACTS_INFO, f)


def store_contract(client: LedgerClient, wallet: LocalWallet, file_path: str, name: str) -> int:
    wasm_byte_code = Path(file_path).read_bytes()
    msg: Message
    if PERMISSIONED_UPLOADER_ADDRESS is not None:
        msg = create_exec_msg(
            msg=MsgStoreCode(sender=PERMISSIONED_UPLOADER_ADDRESS, wasm_byte_code=wasm_byte_code),
            grantee_address=str(wallet.address()),
        )
    else:
        msg = MsgStoreCode(sender=str(wallet.address()), wasm_byte_code=wasm_byte_code)
    store_tx = create_tx(msg=msg, client=client, wallet=wallet, gas_limit=STORE_GAS_LIMIT)
    tx_hash = sha256(store_tx.tx.SerializeToString()).hexdigest()
    print("Tx hash: ", tx_hash)
    resp = broadcast_tx(store_tx)
    contract_code_id = get_attribute_value(resp, "store_code", "code_id")
    print(f"Skip Swap {name} Contract Code ID:", contract_code_id)
    DEPLOYED_CONTRACTS_INFO["code-ids"][f"{name}_contract_code_id"] = contract_code_id
    DEPLOYED_CONTRACTS_INFO["tx-hashes"][f"store_{name}_tx_hash"] = tx_hash
    save_deployed_contracts_info()
    return int(contract_code_id)


def instantiate_contract(
    client: LedgerClient,
    wallet: LocalWallet,
    code_id: int,
    args: dict[str, str],
    label: str,
    name: str,
) -> str:
    msg = MsgInstantiateContract(
        sender=str(wallet.address()),
        code_id=code_id,
        msg=json.dumps(args).encode("UTF8"),
        label=label,
    )
    instantiate_tx = create_tx(
        msg=msg, client=client, wallet=wallet, gas_limit=INSTANTIATE_GAS_LIMIT
    )
    tx_hash = sha256(instantiate_tx.tx.SerializeToString()).hexdigest()
    print("Tx hash: ", tx_hash)
    resp = broadcast_tx(instantiate_tx)
    contract_address = get_attribute_value(resp, "instantiate", "_contract_address")
    print(f"Skip Swap {name} Contract Address:", contract_address)
    DEPLOYED_CONTRACTS_INFO["contract-addresses"][f"{name}_contract_address"] = contract_address
    DEPLOYED_CONTRACTS_INFO["tx-hashes"][f"instantiate_{name}_tx_hash"] = tx_hash
    save_deployed_contracts_info()
    return contract_address


def instantiate2_contract(
    client: LedgerClient,
    wallet: LocalWallet,
    code_id: int,
    args: dict[str, str | list[dict[str, str]]],
    label: str,
    name: str,
) -> str:
    msg = MsgInstantiateContract2(
        sender=str(wallet.address()),
        code_id=code_id,
        msg=json.dumps(args).encode("UTF8"),
        label=label,
        salt=SALT,
        fix_msg=False,
    )
    instantiate_2_tx = create_tx(
        msg=msg, client=client, wallet=wallet, gas_limit=INSTANTIATE_GAS_LIMIT
    )
    tx_hash = sha256(instantiate_2_tx.tx.SerializeToString()).hexdigest()
    print("Tx hash: ", tx_hash)
    resp = broadcast_tx(instantiate_2_tx)
    contract_address = get_attribute_value(resp, "instantiate", "_contract_address")
    print(f"Skip Swap {name} Contract Address:", contract_address)
    DEPLOYED_CONTRACTS_INFO["contract-addresses"][f"{name}_contract_address"] = contract_address
    DEPLOYED_CONTRACTS_INFO["tx-hashes"][f"instantiate_{name}_tx_hash"] = tx_hash
    save_deployed_contracts_info()
    return contract_address


def create_any_msg(msg: Message) -> ProtoAny:
    any_msg = ProtoAny()
    _ = any_msg.Pack(msg, "")
    return any_msg


def create_exec_msg(msg: Message, grantee_address: str) -> MsgExec:
    return MsgExec(grantee=grantee_address, msgs=[create_any_msg(msg)])


def broadcast_tx(tx: Transaction) -> httpx.Response:
    tx_bytes = tx.tx.SerializeToString()
    encoded_tx = b64encode(tx_bytes).decode("utf-8")
    data = {"jsonrpc": "2.0", "method": "broadcast_tx_sync", "params": [encoded_tx], "id": 1}
    post_resp = httpx.post(RPC_URL, json=data, timeout=60)
    print("broadcast response: ", post_resp.text)
    print("Sleeping for 20 seconds...")
    time.sleep(20)
    return httpx.get(
        REST_URL + f"/cosmos/tx/v1beta1/txs/{sha256(tx_bytes).hexdigest()}", timeout=60
    )


def get_attribute_value(resp: httpx.Response, event_type: str, attr_key: str) -> str:
    tx_response = cast(GetTxResponse, resp.json())["tx_response"]
    logs = tx_response.get("logs", [])
    events = logs[0]["events"] if logs else tx_response["events"]

    for event in events:
        if event["type"] == event_type:
            for attr in event["attributes"]:
                if attr["key"] == attr_key:
                    return attr["value"]
    raise DeployError(f"Attribute {attr_key!r} of event {event_type!r} not found in tx response")


if __name__ == "__main__":
    main()
