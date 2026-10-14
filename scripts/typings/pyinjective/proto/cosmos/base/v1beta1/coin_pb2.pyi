"""
Hand-written partial stub: only the messages used by deploy_injective.py.
"""

from _typeshed import Incomplete
from google.protobuf.message import Message

class Coin(Message):
    denom: str
    amount: str
    def __init__(self, *, denom: str | None = ..., amount: str | None = ...) -> None: ...

def __getattr__(name: str) -> Incomplete: ...
