CREATE TABLE invoices (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    payment_hash BLOB NOT NULL UNIQUE,
    preimage BLOB,
    bolt11 TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE htlcs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    invoice_id INTEGER REFERENCES invoices (id),
    state TEXT NOT NULL,
    scid TEXT NOT NULL,
    channel_id INTEGER NOT NULL,
    msat INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
