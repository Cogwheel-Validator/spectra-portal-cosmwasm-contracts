"""
Hand-written partial stub: only the messages used by deploy_injective.py.
"""

from _typeshed import Incomplete
from google.protobuf.message import Message

class SignDoc(Message):
    body_bytes: bytes
    auth_info_bytes: bytes
    chain_id: str
    account_number: int

def __getattr__(name: str) -> Incomplete: ...
