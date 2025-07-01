import time
from collections.abc import Generator

import grpc
import pytest

from hold.protos.hold_pb2 import (
    CancelRequest,
    InvoiceRequest,
    InvoiceResponse,
    SettleRequest,
)
from hold.protos.hold_pb2_grpc import HoldStub
from hold.utils import LndPay, hold_client, new_preimage_bytes


class TestState:
    @pytest.fixture(scope="class", autouse=True)
    def cl(self) -> Generator[HoldStub, None, None]:
        (channel, client) = hold_client()

        yield client

        channel.close()

    def test_invoice_settle_unpaid(self, cl: HoldStub) -> None:
        (preimage, payment_hash) = new_preimage_bytes()
        cl.Invoice(InvoiceRequest(payment_hash=payment_hash, amount_msat=1_000))

        with pytest.raises(Exception) as e:
            cl.Settle(SettleRequest(payment_preimage=preimage))

        assert e.value.code() == grpc.StatusCode.INTERNAL
        assert e.value.details() == "could not settle invoice: no HTLCs to settle"

    def test_invoice_cancel_paid(self, cl: HoldStub) -> None:
        (preimage, payment_hash) = new_preimage_bytes()
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(payment_hash=payment_hash, amount_msat=1_000)
        )

        pay = LndPay(1, invoice.bolt11)
        pay.start()
        time.sleep(1)

        cl.Settle(SettleRequest(payment_preimage=preimage))

        with pytest.raises(Exception) as e:
            cl.Cancel(CancelRequest(payment_hash=payment_hash))

        assert e.value.code() == grpc.StatusCode.INTERNAL
        assert (
            e.value.details() == "could not cancel invoice: "
            "could not update invoice in database: state paid is final"
        )
