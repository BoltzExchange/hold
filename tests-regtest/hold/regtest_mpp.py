import json
import time

import bolt11
import pytest
from bolt11 import MilliSatoshi

from hold.protos.hold_pb2 import (
    Invoice,
    InvoiceRequest,
    InvoiceResponse,
    InvoiceState,
    ListRequest,
    SettleRequest,
)
from hold.protos.hold_pb2_grpc import HoldStub
from hold.utils import LndPay, hold_client, lightning, new_preimage_bytes


class TestMpp:
    @pytest.fixture(scope="class", autouse=True)
    def cl(self) -> HoldStub:
        (channel, client) = hold_client()

        yield client

        channel.close()

    @pytest.mark.parametrize("parts", [2, 4, 5, 10])
    def test_mpp_payment(self, cl: HoldStub, parts: int) -> None:
        (preimage, payment_hash) = new_preimage_bytes()
        amount = 20_000
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(payment_hash=payment_hash, amount_msat=amount)
        )

        shard_size = int(amount / parts / 1_000)
        pay = LndPay(1, invoice.bolt11, max_shard_size=shard_size)
        pay.start()

        # 10 parts can take a little longer than a second
        time.sleep(2)
        info: Invoice = cl.List(ListRequest(payment_hash=payment_hash)).invoices[0]
        assert info.state == InvoiceState.ACCEPTED
        assert len(info.htlcs) == parts
        assert all(htlc.state == InvoiceState.ACCEPTED for htlc in info.htlcs)
        assert all(htlc.msat == int(amount / parts) for htlc in info.htlcs)

        cl.Settle(SettleRequest(payment_preimage=preimage))

        pay.join()
        assert pay.res["status"] == "SUCCEEDED"

    def test_mpp_timeout(self, cl: HoldStub) -> None:
        (_, payment_hash) = new_preimage_bytes()
        amount = 20_000
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(payment_hash=payment_hash, amount_msat=amount)
        )

        dec = bolt11.decode(invoice.bolt11)
        dec.amount_msat = MilliSatoshi(dec.amount_msat - 1_000)

        pay = LndPay(
            1, lightning("signinvoice", bolt11.encode(dec))["bolt11"], timeout=1
        )
        pay.start()
        pay.join()

        assert pay.res["status"] == "FAILED"
        assert pay.res["failure_reason"] == "FAILURE_REASON_TIMEOUT"
        assert len(pay.res["htlcs"]) == 1

        htlc = pay.res["htlcs"][0]
        assert htlc["failure"]["code"] == "MPP_TIMEOUT"

        list_invoice: Invoice = cl.List(
            ListRequest(
                payment_hash=payment_hash,
            )
        ).invoices[0]
        assert list_invoice.state == InvoiceState.UNPAID
        assert len(list_invoice.htlcs) == 1
        assert list_invoice.htlcs[0].state == InvoiceState.CANCELLED

        # Poor man's way to check if there is a pending HTLC for that hash
        assert payment_hash.hex() not in json.dumps(lightning("listpeerchannels"))
