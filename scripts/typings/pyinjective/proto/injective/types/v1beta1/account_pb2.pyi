"""
Hand-written partial stub: only the messages used by deploy_injective.py.
"""

from _typeshed import Incomplete
from google.protobuf.message import Message
from pyinjective.proto.cosmos.auth.v1beta1.auth_pb2 import BaseAccount

class EthAccount(Message):
    @property
    def base_account(self) -> BaseAccount: ...
    code_hash: bytes

def __getattr__(name: str) -> Incomplete: ...
