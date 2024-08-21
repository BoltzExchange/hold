from __future__ import annotations

import json
import os
from datetime import datetime, timezone
from hashlib import sha256
from pathlib import Path
from threading import Thread
from typing import Any

import grpc

from hold.protos.hold_pb2_grpc import HoldStub


def time_now() -> datetime:
    return datetime.now(tz=timezone.utc)


def new_preimage_bytes() -> tuple[bytes, bytes]:
    preimage = os.urandom(32)
    return preimage, sha256(preimage).digest()


def new_preimage() -> tuple[str, str]:
    preimage = os.urandom(32)
    return preimage.hex(), sha256(preimage).hexdigest()


def hold_client() -> tuple[grpc.Channel, HoldStub]:
    cert_path = Path("../regtest/data/cln2/regtest/hold")
    creds = grpc.ssl_channel_credentials(
        root_certificates=cert_path.joinpath("ca.pem").read_bytes(),
        private_key=cert_path.joinpath("client-key.pem").read_bytes(),
        certificate_chain=cert_path.joinpath("client.pem").read_bytes(),
    )
    channel = grpc.secure_channel(
        "127.0.0.1:9738",
        creds,
        options=(("grpc.ssl_target_name_override", "hold"),),
    )
    client = HoldStub(channel)

    return channel, client


def lightning(*args: str, node: int = 2) -> dict[str, Any]:
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
