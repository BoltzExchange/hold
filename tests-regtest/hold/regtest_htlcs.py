import json
import time

import bolt11
import pytest
from bolt11 import MilliSatoshi
from bolt11.models.tags import TagChar

from hold.protos.hold_pb2 import (
    Invoice,
    InvoiceRequest,
    InvoiceResponse,
    InvoiceState,
    ListRequest,
    SettleRequest,
)
from hold.protos.hold_pb2_grpc import HoldStub
from hold.utils import (
    LndPay,
    hold_client,
    lightning,
    lnd,
    new_preimage,
    new_preimage_bytes,
)


def assert_failed_payment(
    cl: HoldStub,
    payment_hash: bytes,
    dec: bolt11.Bolt11,
    reason: str = "FAILURE_REASON_INCORRECT_PAYMENT_DETAILS",
) -> None:
    invoice = lightning("signinvoice", bolt11.encode(dec))["bolt11"]

    pay = LndPay(1, invoice)
    pay.start()
    pay.join()

    assert pay.res["status"] == "FAILED"
    assert pay.res["failure_reason"] == reason

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


class TestHtlcs:
    @pytest.fixture(scope="class", autouse=True)
    def cl(self) -> HoldStub:
        (channel, client) = hold_client()

        yield client

        channel.close()

    def test_ignore_non_hold_invoice(self) -> None:
        invoice = lightning("invoice", "1000", new_preimage()[0], "invoice-test")[
            "bolt11"
        ]

        pay = LndPay(1, invoice)
        pay.start()
        pay.join()

        assert pay.res["status"] == "SUCCEEDED"

    def test_ignore_forward(self) -> None:
        hold_node_id = lightning("getinfo")["id"]
        outgoing_channel = next(
            c
            for c in lnd("listchannels")["channels"]
            if c["remote_pubkey"] == hold_node_id
        )["scid"]

        invoice = lnd("addinvoice", "1000", node=2)["payment_request"]

        pay = LndPay(1, invoice, outgoing_chan_id=outgoing_channel)
        pay.start()
        pay.join()

        assert pay.res["status"] == "SUCCEEDED"

    def test_invalid_payment_secret(self, cl: HoldStub) -> None:
        (_, payment_hash) = new_preimage_bytes()
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(payment_hash=payment_hash, amount_msat=21_000)
        )

        dec = bolt11.decode(invoice.bolt11)
        dec.tags.get(TagChar.payment_secret).data = new_preimage()[0]

        assert_failed_payment(cl, payment_hash, dec)

    def test_invalid_final_cltv_expiry(self, cl: HoldStub) -> None:
        (_, payment_hash) = new_preimage_bytes()
        min_final_cltv_expiry = 80
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(
                payment_hash=payment_hash,
                amount_msat=21_000,
                min_final_cltv_expiry=min_final_cltv_expiry,
            )
        )

        dec = bolt11.decode(invoice.bolt11)
        dec.tags.get(TagChar.min_final_cltv_expiry).data = min_final_cltv_expiry - 21

        assert_failed_payment(cl, payment_hash, dec, "FAILURE_REASON_ERROR")

    def test_acceptable_overpayment(self, cl: HoldStub) -> None:
        (preimage, payment_hash) = new_preimage_bytes()
        amount = 21_000
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(
                payment_hash=payment_hash,
                amount_msat=amount,
            )
        )

        dec = bolt11.decode(invoice.bolt11)
        dec.amount_msat = MilliSatoshi(amount * 2)

        invoice_signed = lightning("signinvoice", bolt11.encode(dec))["bolt11"]
        pay = LndPay(1, invoice_signed)
        pay.start()

        time.sleep(1)
        cl.Settle(SettleRequest(payment_preimage=preimage))

        pay.join()
        assert pay.res["status"] == "SUCCEEDED"

    def test_unacceptable_overpayment(self, cl: HoldStub) -> None:
        (preimage, payment_hash) = new_preimage_bytes()
        amount = 21_000
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(
                payment_hash=payment_hash,
                amount_msat=amount,
            )
        )

        dec = bolt11.decode(invoice.bolt11)
        dec.amount_msat = MilliSatoshi((amount * 2) + 1)

        assert_failed_payment(cl, payment_hash, dec)
