from __future__ import annotations

import time
from typing import Any

from hold.utils import LndPay, lightning, new_preimage


def check_unpaid_invoice(
    entry: dict[str, Any], payment_hash: str, invoice: str
) -> None:
    assert entry is not None
    assert entry["payment_hash"] == payment_hash
    assert entry["preimage"] is None
    assert entry["bolt11"] == invoice
    assert entry["state"] == "unpaid"
    assert len(entry["htlcs"]) == 0


class TestRpc:
    def test_invoice(self) -> None:
        amount = 2_112
        (_, payment_hash) = new_preimage()

        invoice = lightning("holdinvoice", payment_hash, f"{amount}")
        decoded = lightning("decode", invoice["bolt11"])

        assert decoded["valid"]
        assert decoded["amount_msat"] == amount
        assert decoded["payment_hash"] == payment_hash
        assert decoded["payee"] == lightning("getinfo")["id"]
        assert "payment_secret" in decoded

    def test_list(self) -> None:
        (_, payment_hash) = new_preimage()
        invoice = lightning("holdinvoice", payment_hash, "1")["bolt11"]

        list_all = lightning("listholdinvoices")["holdinvoices"]

        assert len(list_all) > 1

        entry = next(e for e in list_all if e["bolt11"] == invoice)
        assert entry is not None
        check_unpaid_invoice(entry, payment_hash, invoice)

    def test_list_payment_hash(self) -> None:
        (_, payment_hash) = new_preimage()
        invoice = lightning("holdinvoice", payment_hash, "1")["bolt11"]

        list_entries = lightning("listholdinvoices", payment_hash)["holdinvoices"]
        assert len(list_entries) == 1
        check_unpaid_invoice(list_entries[0], payment_hash, invoice)

    def test_list_invoice(self) -> None:
        (_, payment_hash) = new_preimage()
        invoice = lightning("holdinvoice", payment_hash, "1")["bolt11"]

        list_entries = lightning("listholdinvoices", "null", invoice)["holdinvoices"]
        assert len(list_entries) == 1
        check_unpaid_invoice(list_entries[0], payment_hash, invoice)

    def test_settle(self) -> None:
        amount = 1_000
        (preimage, payment_hash) = new_preimage()

        invoice = lightning("holdinvoice", payment_hash, f"{amount}")["bolt11"]

        payer = LndPay(1, invoice)
        payer.start()
        time.sleep(1)

        data = lightning("listholdinvoices", payment_hash)["holdinvoices"][0]
        assert data["state"] == "accepted"

        htlcs = data["htlcs"]
        assert len(htlcs) == 1
        assert htlcs[0]["state"] == "accepted"
        assert htlcs[0]["msat"] == amount

        lightning("settleholdinvoice", preimage)

        payer.join()
        assert payer.res["status"] == "SUCCEEDED"

        data = lightning("listholdinvoices", payment_hash)["holdinvoices"][0]
        assert data["state"] == "paid"

        htlcs = data["htlcs"]
        assert len(htlcs) == 1
        assert htlcs[0]["state"] == "paid"

    def test_cancel(self) -> None:
        (_, payment_hash) = new_preimage()
        invoice = lightning("holdinvoice", payment_hash, "1000")["bolt11"]

        payer = LndPay(1, invoice)
        payer.start()
        time.sleep(1)

        data = lightning("listholdinvoices", payment_hash)["holdinvoices"][0]
        assert data["state"] == "accepted"

        htlcs = data["htlcs"]
        assert len(htlcs) == 1
        assert htlcs[0]["state"] == "accepted"

        lightning("cancelholdinvoice", payment_hash)

        payer.join()
        assert payer.res["status"] == "FAILED"

        data = lightning("listholdinvoices", payment_hash)["holdinvoices"][0]
        assert data["state"] == "cancelled"

        htlcs = data["htlcs"]
        assert len(htlcs) == 1
        assert htlcs[0]["state"] == "cancelled"
