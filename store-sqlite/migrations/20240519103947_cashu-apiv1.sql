-- Add migration script here

-- 
ALTER TABLE proofs ADD COLUMN witness TEXT;
ALTER TABLE proofs ADD COLUMN dleq TEXT;
ALTER TABLE proofs ADD COLUMN unit TEXT;

-- 
ALTER TABLE transactions ADD COLUMN unit TEXT;
