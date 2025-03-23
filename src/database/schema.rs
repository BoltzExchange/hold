diesel::table! {
    invoices (id) {
        id -> BigInt,
        payment_hash -> Binary,
        preimage -> Nullable<Binary>,
        invoice -> Text,
        state -> Text,
        min_cltv -> Nullable<Integer>,
        created_at -> Timestamp,
        settled_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    htlcs (id) {
        id -> BigInt,
        invoice_id -> BigInt,
        state -> Text,
        scid -> Text,
        channel_id -> BigInt,
        msat -> BigInt,
        created_at -> Timestamp,
    }
}

diesel::joinable!(htlcs -> invoices (invoice_id));

diesel::allow_tables_to_appear_in_same_query!(invoices, htlcs,);
