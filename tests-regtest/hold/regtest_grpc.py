from __future__ import annotations

import concurrent.futures
import time

import pytest

from hold.protos.hold_pb2 import (
    CancelRequest,
    CleanRequest,
    GetInfoRequest,
    GetInfoResponse,
    Hop,
    Invoice,
    InvoiceRequest,
    InvoiceResponse,
    InvoiceState,
    ListRequest,
    ListResponse,
    RoutingHint,
    SettleRequest,
    TrackAllRequest,
    TrackRequest,
)
from hold.protos.hold_pb2_grpc import HoldStub
from hold.utils import LndPay, hold_client, lightning, new_preimage_bytes, time_now


class TestGrpc:
    @pytest.fixture(scope="class", autouse=True)
    def cl(self) -> HoldStub:
        (channel, client) = hold_client()

        yield client

        channel.close()

    def test_get_info(self, cl: HoldStub) -> None:
        info: GetInfoResponse = cl.GetInfo(GetInfoRequest())
        assert info.version != ""

    def test_invoice_defaults(self, cl: HoldStub) -> None:
        amount = 21_000
        (_, payment_hash) = new_preimage_bytes()

        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(payment_hash=payment_hash, amount_msat=amount)
        )
        decoded = lightning("decode", invoice.bolt11)

        assert decoded["currency"] == "bcrt"
        assert decoded["created_at"] - int(time_now().timestamp()) < 2
        assert decoded["expiry"] == 3_600
        assert decoded["payee"] == lightning("getinfo")["id"]
        assert decoded["amount_msat"] == amount
        assert decoded["description"] == ""
        assert decoded["min_final_cltv_expiry"] == 80
        assert "payment_secret" in decoded
        assert decoded["features"] == "024100"
        assert decoded["payment_hash"] == payment_hash.hex()
        assert decoded["valid"]

    @pytest.mark.parametrize(
        "memo",
        [
            "some",
            "text",
            "Send to BTC address",
            "some way longer text with so many chars",
        ],
    )
    def test_invoice_memo(self, cl: HoldStub, memo: str) -> None:
        (_, payment_hash) = new_preimage_bytes()
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(payment_hash=payment_hash, amount_msat=1, memo=memo)
        )
        decoded = lightning("decode", invoice.bolt11)

        assert decoded["description"] == memo

    def test_invoice_description_hash(self, cl: HoldStub) -> None:
        (preimage, payment_hash) = new_preimage_bytes()
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(payment_hash=payment_hash, amount_msat=1, hash=preimage)
        )
        decoded = lightning("decode", invoice.bolt11)

        assert decoded["description_hash"] == preimage.hex()

    @pytest.mark.parametrize(
        "expiry",
        [
            1,
            2,
            3_600,
            7_200,
            10_000,
        ],
    )
    def test_invoice_expiry(self, cl: HoldStub, expiry: int) -> None:
        (_, payment_hash) = new_preimage_bytes()
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(payment_hash=payment_hash, amount_msat=1, expiry=expiry)
        )
        decoded = lightning("decode", invoice.bolt11)

        assert decoded["expiry"] == expiry

    @pytest.mark.parametrize(
        "expiry",
        [
            1,
            2,
            80,
            144,
            288,
        ],
    )
    def test_invoice_min_final_cltv_expiry(self, cl: HoldStub, expiry: int) -> None:
        (_, payment_hash) = new_preimage_bytes()
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(
                payment_hash=payment_hash, amount_msat=1, min_final_cltv_expiry=expiry
            )
        )
        decoded = lightning("decode", invoice.bolt11)

        assert decoded["min_final_cltv_expiry"] == expiry

    def test_invoice_routing_hints(self, cl: HoldStub) -> None:
        (_, payment_hash) = new_preimage_bytes()

        hints = [
            RoutingHint(
                hops=[
                    Hop(
                        public_key=bytes.fromhex(
                            "026165850492521f4ac8abd9bd8088123446d126f648ca35e60f88177dc149ceb2"
                        ),
                        short_channel_id=123,
                        base_fee=1,
                        ppm_fee=2,
                        cltv_expiry_delta=23,
                    ),
                    Hop(
                        public_key=bytes.fromhex(
                            "02d96eadea3d780104449aca5c93461ce67c1564e2e1d73225fa67dd3b997a6018"
                        ),
                        short_channel_id=321,
                        base_fee=2,
                        ppm_fee=21,
                        cltv_expiry_delta=26,
                    ),
                ]
            ),
            RoutingHint(
                hops=[
                    Hop(
                        public_key=bytes.fromhex(
                            "027a7666ec63448bacaec5b00398dd263522755e95bcded7b52b2c9dc4533d34f1"
                        ),
                        short_channel_id=121,
                        base_fee=1_000,
                        ppm_fee=2_500,
                        cltv_expiry_delta=80,
                    )
                ]
            ),
        ]

        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(
                payment_hash=payment_hash,
                amount_msat=1,
                routing_hints=hints,
            )
        )
        decoded = lightning("decode", invoice.bolt11)
        routes = decoded["routes"]

        assert len(routes) == 2

        assert len(routes[0]) == 2
        assert len(routes[1]) == 1

        for i in range(len(routes)):
            for j in range(len(routes[i])):
                decoded_hop = routes[i][j]
                hint = hints[i].hops[j]

                assert decoded_hop["pubkey"] == hint.public_key.hex()
                assert decoded_hop["short_channel_id"] == f"0x0x{hint.short_channel_id}"
                assert decoded_hop["fee_base_msat"] == hint.base_fee
                assert decoded_hop["fee_proportional_millionths"] == hint.ppm_fee
                assert decoded_hop["cltv_expiry_delta"] == hint.cltv_expiry_delta

    def test_list_all(self, cl: HoldStub) -> None:
        cl.Invoice(InvoiceRequest(payment_hash=new_preimage_bytes()[1], amount_msat=1))

        hold_list: ListResponse = cl.List(ListRequest())
        assert len(hold_list.invoices) > 0

    def test_list_payment_hash(self, cl: HoldStub) -> None:
        (_, payment_hash) = new_preimage_bytes()
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(payment_hash=payment_hash, amount_msat=1)
        )

        hold_list: ListResponse = cl.List(ListRequest(payment_hash=payment_hash))
        assert len(hold_list.invoices) == 1

        assert hold_list.invoices[0].bolt11 == invoice.bolt11
        assert hold_list.invoices[0].payment_hash == payment_hash

    def test_list_payment_hash_not_found(self, cl: HoldStub) -> None:
        (_, payment_hash) = new_preimage_bytes()

        hold_list: ListResponse = cl.List(ListRequest(payment_hash=payment_hash))
        assert len(hold_list.invoices) == 0

    def test_list_pagination(self, cl: HoldStub) -> None:
        for _ in range(10):
            (_, payment_hash) = new_preimage_bytes()
            cl.Invoice(InvoiceRequest(payment_hash=payment_hash, amount_msat=1))

        page: ListResponse = cl.List(
            ListRequest(pagination=ListRequest.Pagination(index_start=0, limit=2))
        )
        assert len(page.invoices) == 2
        assert page.invoices[0].id == 1
        assert page.invoices[1].id == 2

        page: ListResponse = cl.List(
            ListRequest(pagination=ListRequest.Pagination(index_start=2, limit=1))
        )
        assert len(page.invoices) == 1
        assert page.invoices[0].id == 2

        page: ListResponse = cl.List(
            ListRequest(pagination=ListRequest.Pagination(index_start=3, limit=5))
        )
        assert len(page.invoices) == 5
        assert page.invoices[0].id == 3

    def test_clean_cancelled(self, cl: HoldStub) -> None:
        # One that we are not going to cancel which should not be cleaned
        (_, payment_hash) = new_preimage_bytes()
        cl.Invoice(InvoiceRequest(payment_hash=payment_hash, amount_msat=1))

        (_, payment_hash) = new_preimage_bytes()
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(payment_hash=payment_hash, amount_msat=1_000)
        )

        pay = LndPay(1, invoice.bolt11)
        pay.start()
        time.sleep(1)

        cl.Cancel(CancelRequest(payment_hash=payment_hash))
        pay.join()

        res = cl.Clean(CleanRequest(age=0))
        assert res.cleaned > 0

        res = cl.List(ListRequest(payment_hash=payment_hash))
        assert len(res.invoices) == 0

        res = cl.List(ListRequest())
        assert len(res.invoices) > 0

    def test_track_settle(self, cl: HoldStub) -> None:
        (preimage, payment_hash) = new_preimage_bytes()
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(payment_hash=payment_hash, amount_msat=1_000)
        )

        def track_states() -> list[InvoiceState]:
            return [
                update.state
                for update in cl.Track(TrackRequest(payment_hash=payment_hash))
            ]

        with concurrent.futures.ThreadPoolExecutor() as pool:
            fut = pool.submit(track_states)

            pay = LndPay(1, invoice.bolt11)
            pay.start()
            time.sleep(1)

            invoice_state: Invoice = cl.List(
                ListRequest(payment_hash=payment_hash)
            ).invoices[0]
            assert invoice_state.state == InvoiceState.ACCEPTED
            assert len(invoice_state.htlcs) == 1
            assert invoice_state.htlcs[0].state == InvoiceState.ACCEPTED

            cl.Settle(SettleRequest(payment_preimage=preimage))
            pay.join()

            assert fut.result() == [
                InvoiceState.UNPAID,
                InvoiceState.ACCEPTED,
                InvoiceState.PAID,
            ]

            invoice_state = cl.List(ListRequest(payment_hash=payment_hash)).invoices[0]
            assert invoice_state.state == InvoiceState.PAID
            assert len(invoice_state.htlcs) == 1
            assert invoice_state.htlcs[0].state == InvoiceState.PAID

    def test_track_cancel(self, cl: HoldStub) -> None:
        (_, payment_hash) = new_preimage_bytes()
        invoice: InvoiceResponse = cl.Invoice(
            InvoiceRequest(payment_hash=payment_hash, amount_msat=1_000)
        )

        def track_states() -> list[InvoiceState]:
            return [
                update.state
                for update in cl.Track(TrackRequest(payment_hash=payment_hash))
            ]

        with concurrent.futures.ThreadPoolExecutor() as pool:
            fut = pool.submit(track_states)

            pay = LndPay(1, invoice.bolt11)
            pay.start()
            time.sleep(1)

            invoice_state: Invoice = cl.List(
                ListRequest(payment_hash=payment_hash)
            ).invoices[0]
            assert invoice_state.state == InvoiceState.ACCEPTED
            assert len(invoice_state.htlcs) == 1
            assert invoice_state.htlcs[0].state == InvoiceState.ACCEPTED

            cl.Cancel(CancelRequest(payment_hash=payment_hash))
            pay.join()

            assert fut.result() == [
                InvoiceState.UNPAID,
                InvoiceState.ACCEPTED,
                InvoiceState.CANCELLED,
            ]

            invoice_state = cl.List(ListRequest(payment_hash=payment_hash)).invoices[0]
            assert invoice_state.state == InvoiceState.CANCELLED
            assert len(invoice_state.htlcs) == 1
            assert invoice_state.htlcs[0].state == InvoiceState.CANCELLED

    def test_track_all(self, cl: HoldStub) -> None:
        expected_events = 6

        def track_states() -> list[tuple[bytes, str, str]]:
            evs = []

            sub = cl.TrackAll(TrackAllRequest())
            for ev in sub:
                evs.append((ev.payment_hash, ev.bolt11, ev.state))
                if len(evs) == expected_events:
                    sub.cancel()
                    break

            return evs

        with concurrent.futures.ThreadPoolExecutor() as pool:
            fut = pool.submit(track_states)

            (_, payment_hash_created) = new_preimage_bytes()
            invoice_created: InvoiceResponse = cl.Invoice(
                InvoiceRequest(payment_hash=payment_hash_created, amount_msat=1_000)
            )

            (_, payment_hash_cancelled) = new_preimage_bytes()
            invoice_cancelled: InvoiceResponse = cl.Invoice(
                InvoiceRequest(payment_hash=payment_hash_cancelled, amount_msat=1_000)
            )

            (preimage_settled, payment_hash_settled) = new_preimage_bytes()
            invoice_settled: InvoiceResponse = cl.Invoice(
                InvoiceRequest(payment_hash=payment_hash_settled, amount_msat=1_000)
            )

            cl.Cancel(CancelRequest(payment_hash=payment_hash_cancelled))

            pay = LndPay(1, invoice_settled.bolt11)
            pay.start()
            time.sleep(1)

            cl.Settle(SettleRequest(payment_preimage=preimage_settled))
            pay.join()

            res = fut.result()
            assert len(res) == expected_events
            assert res == [
                (payment_hash_created, invoice_created.bolt11, InvoiceState.UNPAID),
                (payment_hash_cancelled, invoice_cancelled.bolt11, InvoiceState.UNPAID),
                (payment_hash_settled, invoice_settled.bolt11, InvoiceState.UNPAID),
                (
                    payment_hash_cancelled,
                    invoice_cancelled.bolt11,
                    InvoiceState.CANCELLED,
                ),
                (payment_hash_settled, invoice_settled.bolt11, InvoiceState.ACCEPTED),
                (payment_hash_settled, invoice_settled.bolt11, InvoiceState.PAID),
            ]

    def test_track_all_existing(self, cl: HoldStub) -> None:
        expected_events = 3

        (_, payment_hash_not_found) = new_preimage_bytes()
        (preimage_settled, payment_hash_settled) = new_preimage_bytes()

        def track_states() -> list[tuple[bytes, str, str]]:
            evs = []

            sub = cl.TrackAll(
                TrackAllRequest(
                    payment_hashes=[payment_hash_not_found, payment_hash_settled]
                )
            )
            for ev in sub:
                evs.append((ev.payment_hash, ev.bolt11, ev.state))
                if len(evs) == expected_events:
                    sub.cancel()
                    break

            return evs

        with concurrent.futures.ThreadPoolExecutor() as pool:
            invoice_settled: InvoiceResponse = cl.Invoice(
                InvoiceRequest(payment_hash=payment_hash_settled, amount_msat=1_000)
            )

            fut = pool.submit(track_states)

            pay = LndPay(1, invoice_settled.bolt11)
            pay.start()
            time.sleep(1)

            cl.Settle(SettleRequest(payment_preimage=preimage_settled))
            pay.join()

            res = fut.result()
            assert len(res) == expected_events
            assert res == [
                (payment_hash_settled, invoice_settled.bolt11, InvoiceState.UNPAID),
                (payment_hash_settled, invoice_settled.bolt11, InvoiceState.ACCEPTED),
                (payment_hash_settled, invoice_settled.bolt11, InvoiceState.PAID),
            ]
