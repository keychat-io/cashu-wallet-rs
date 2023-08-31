-- Add migration script here

-- https://github.com/cashubtc/nuts/blob/main/13.md
CREATE TABLE IF NOT EXISTS counters (
    mint TEXT NOT NULL,
    keysetid TEXT NOT NULL,
    counter TEXT NOT NULL,
    ctime bigint NOT NULL,
    pubkey TEXT NOT NULL,
    UNIQUE (mint, keysetid, pubkey)
);
