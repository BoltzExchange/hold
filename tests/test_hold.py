from __future__ import annotations

from hashlib import sha256
from pathlib import Path
from threading import Thread

from pyln.client import RpcError
from pyln.testing.fixtures import *


def new_preimage() -> tuple[str, str]:
    preimage = os.urandom(32)
    return preimage.hex(), sha256(preimage).hexdigest()


@pytest.fixture
def plugin_path() -> Path:
    return Path.cwd() / "tests" / "build" / "hold-linux-amd64"


def pay_with_thread(rpc: any, bolt11: str) -> None:
    logger = logging.getLogger(__name__)
    try:
        rpc.dev_pay(bolt11, dev_use_shadow=False)
    except RpcError as e:
        logger.info("error paying invoice with payment hash: %s", e)


def test_holdinvoice(node_factory: NodeFactory, plugin_path: Path) -> None:
    node = node_factory.get_node(
        options={"important-plugin": plugin_path, "hold-grpc-port": "-1"}
    )

    amount = 1_000
    _, payment_hash = new_preimage()
    res = node.rpc.call(
        "holdinvoice",
        {
            "amount": amount,
            "payment_hash": payment_hash,
        },
    )

    decoded = node.rpc.call("decode", [res["bolt11"]])

    assert decoded["payment_hash"] == payment_hash
    assert decoded["amount_msat"] == amount


def test_listholdinvoices(node_factory: NodeFactory, plugin_path: Path) -> None:
    node = node_factory.get_node(
        options={"important-plugin": plugin_path, "hold-grpc-port": "-1"}
    )

    hashes = [new_preimage()[0] for _ in range(5)]
    invoices = [
        node.rpc.call(
            "holdinvoice",
            {
                "amount": 1_000,
                "payment_hash": payment_hash,
            },
        )["bolt11"]
        for payment_hash in hashes
    ]

    assert len(node.rpc.call("listholdinvoices")["holdinvoices"]) == len(hashes)
    assert node.rpc.call(
        "listholdinvoices", {"payment_hash": hashes[0]}
    ) == node.rpc.call("listholdinvoices", {"bolt11": invoices[0]})


def test_settle(
    node_factory: NodeFactory, bitcoind: BitcoinD, plugin_path: Path
) -> None:
    l1 = node_factory.get_node(
        options={"important-plugin": plugin_path, "hold-grpc-port": "-1"}
    )
    l2 = node_factory.get_node()

    l1.rpc.connect(l2.info["id"], "localhost", l2.port)
    cl1, _ = l1.fundchannel(l2, 1_000_000)
    cl2, _ = l2.fundchannel(l1, 1_000_000)

    bitcoind.generate_block(6)

    l1.wait_channel_active(cl1)
    l1.wait_channel_active(cl2)

    preimage, payment_hash = new_preimage()
    amount = 1_000
    invoice = l1.rpc.call(
        "holdinvoice", {"amount": amount, "payment_hash": payment_hash}
    )["bolt11"]

    Thread(target=pay_with_thread, args=(l2, invoice)).start()
    time.sleep(2)

    assert (
        l1.rpc.call(
            "listholdinvoices",
            {
                "payment_hash": payment_hash,
            },
        )["holdinvoices"][0]["state"]
        == "accepted"
    )

    l1.rpc.call("settleholdinvoice", {"preimage": preimage})

    assert (
        l1.rpc.call(
            "listholdinvoices",
            {
                "payment_hash": payment_hash,
            },
        )["holdinvoices"][0]["state"]
        == "paid"
    )


def test_cancel(
    node_factory: NodeFactory, bitcoind: BitcoinD, plugin_path: Path
) -> None:
    l1 = node_factory.get_node(
        options={"important-plugin": plugin_path, "hold-grpc-port": "-1"}
    )
    l2 = node_factory.get_node()

    l1.rpc.connect(l2.info["id"], "localhost", l2.port)
    cl1, _ = l1.fundchannel(l2, 1_000_000)
    cl2, _ = l2.fundchannel(l1, 1_000_000)

    bitcoind.generate_block(6)

    l1.wait_channel_active(cl1)
    l1.wait_channel_active(cl2)

    _, payment_hash = new_preimage()
    amount = 1_000
    invoice = l1.rpc.call(
        "holdinvoice", {"amount": amount, "payment_hash": payment_hash}
    )["bolt11"]

    Thread(target=pay_with_thread, args=(l2, invoice)).start()
    time.sleep(2)

    assert (
        l1.rpc.call(
            "listholdinvoices",
            {
                "payment_hash": payment_hash,
            },
        )["holdinvoices"][0]["state"]
        == "accepted"
    )

    l1.rpc.call("cancelholdinvoice", {"payment_hash": payment_hash})

    assert (
        l1.rpc.call(
            "listholdinvoices",
            {
                "payment_hash": payment_hash,
            },
        )["holdinvoices"][0]["state"]
        == "cancelled"
    )
