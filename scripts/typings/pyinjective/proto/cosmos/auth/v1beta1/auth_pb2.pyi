"""
Hand-written partial stub: only the messages used by deploy_injective.py.
"""

from _typeshed import Incomplete
from google.protobuf.message import Message

class BaseAccount(Message):
    address: str
    account_number: int
    sequence: int

def __getattr__(name: str) -> Incomplete: ...
