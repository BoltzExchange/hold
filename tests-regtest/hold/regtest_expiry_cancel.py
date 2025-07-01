import time
from collections.abc import Generator

import pytest

from hold.protos.hold_pb2 import InvoiceRequest, InvoiceState, ListRequest
from hold.protos.hold_pb2_grpc import HoldStub
from hold.utils import (
    LndPay,
    bitcoin_cli,
    hold_client,
    lightning,
    lnd_raw,
    new_preimage_bytes,
)

EXPIRY_DEADLINE = 3


class TestExpiryCancel:
    @pytest.fixture(scope="class", autouse=True)
    def cl(self) -> Generator[HoldStub, None, None]:
        (channel, client) = hold_client()
        lnd_raw("resetmc", node=1)

        yield client

        lnd_raw("resetmc", node=1)
        channel.close()

    def test_expiry_cancel(self, cl: HoldStub) -> None:
        bitcoin_cli("-generate 1")
        self.wait_for_cln_sync()

        (_, payment_hash) = new_preimage_bytes()
        invoice = cl.Invoice(
            InvoiceRequest(
                payment_hash=payment_hash, amount_msat=1_000, min_final_cltv_expiry=5
            )
        )

        pay = LndPay(1, invoice.bolt11)
        pay.start()
        time.sleep(1)

        htlc_expiry = self.find_htlc_with_min_expiry(payment_hash.hex())
        assert htlc_expiry is not None

        best_height = self.get_block_height()
        to_mine = (htlc_expiry - best_height) - EXPIRY_DEADLINE
        bitcoin_cli("-generate", to_mine)

        pay.join()
        assert pay.res["status"] == "FAILED"
        assert pay.res["failure_reason"] == "FAILURE_REASON_INCORRECT_PAYMENT_DETAILS"

        # Make sure the HTLCs are cancelled
        assert self.find_htlc_with_min_expiry(payment_hash.hex()) is None

        assert (
            cl.List(ListRequest(payment_hash=payment_hash)).invoices[0].state
            == InvoiceState.CANCELLED
        )

    def wait_for_cln_sync(self) -> None:
        best_height = self.get_block_height()

        while True:
            if best_height == lightning("getinfo")["blockheight"]:
                break

            time.sleep(0.1)

    def get_block_height(self) -> int:
        return bitcoin_cli("getblockchaininfo")["blocks"]

    def find_htlc_with_min_expiry(self, preimage_hash: str) -> int | None:
        peer_channels = lightning("listpeerchannels")
        expiries = [
            htlc["expiry"]
            for channel in peer_channels["channels"]
            for htlc in channel["htlcs"]
            if htlc["payment_hash"] == preimage_hash
        ]

        if len(expiries) == 0:
            return None

        return min(expiries)
