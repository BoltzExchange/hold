from __future__ import annotations

import json
import os
from hashlib import sha256
from threading import Thread
from typing import Any


def lightning(*args: str, node: int = 1) -> dict[str, Any]:
    return json.load(
        os.popen(
            f"docker exec boltz-cln-{node} lightning-cli --regtest {' '.join(args)}",
        ),
    )


def lnd(*args: str, node: int = 1) -> dict[str, Any]:
    return json.loads(lnd_raw(*args, node=node))


def lnd_raw(*args: str, node: int = 1) -> str:
    return os.popen(
        f"docker exec boltz-lnd-{node} lncli -n regtest {' '.join(args)}"
    ).read()


def new_preimage() -> tuple[str, str]:
    preimage = os.urandom(32)
    return preimage.hex(), sha256(preimage).hexdigest()


class LndPay(Thread):
    res: dict[str, Any] = None

    def __init__(
        self,
        node: int,
        invoice: str,
        max_shard_size: int | None = None,
        outgoing_chan_id: str | None = None,
        timeout: int | None = None,
    ) -> None:
        Thread.__init__(self)

        self.node = node
        self.timeout = timeout
        self.invoice = invoice
        self.max_shard_size = max_shard_size
        self.outgoing_chan_id = outgoing_chan_id

    def run(self) -> None:
        cmd = "payinvoice --force --json"

        if self.outgoing_chan_id is not None:
            cmd += f" --outgoing_chan_id {self.outgoing_chan_id}"

        if self.max_shard_size is not None:
            cmd += f" --max_shard_size_sat {self.max_shard_size}"

        if self.timeout is not None:
            cmd += f" --timeout {self.timeout}s"

        res = lnd_raw(f"{cmd} {self.invoice} 2> /dev/null", node=self.node)
        res = res[res.find("{") :]
        self.res = json.loads(res)
