-- Add migration script here
-- https://www.sqlite.org/datatype3.html
CREATE TABLE IF NOT EXISTS proofs (
    secret TEXT NOT NULL,
    keyset_id TEXT NOT NULL,
    amount bigint NOT NULL,
    c TEXT NOT NULL,
    mint TEXT NOT NULL,
    ctime bigint NOT NULL,
    UNIQUE (secret, mint)
);
CREATE TABLE IF NOT EXISTS mints (
    url TEXT NOT NULL,
    active BOOLEAN NOT NULL,
    info TEXT,
    ctime bigint NOT NULL,
    UNIQUE (url)
);
CREATE TABLE IF NOT EXISTS transactions (
    id TEXT NOT NULL,
    kind TEXT NOT NULL,
    amount bigint NOT NULL,
    status TEXT NOT NULL,
    io TEXT NOT NULL,
    info TEXT,
    ctime bigint NOT NULL,
    -- token base64/Pr bolt11
    token TEXT,
    -- 
    mint TEXT NOT NULL,
    -- 
    fee bigint,
    hash TEXT,
    -- 
    UNIQUE (id, io)
);
CREATE INDEX IF NOT EXISTS index_transactions_ctime ON transactions (ctime);
