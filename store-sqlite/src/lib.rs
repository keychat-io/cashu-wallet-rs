#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate async_trait;

use cashu_wallet::cashu::nuts::nut01::PublicKey;
use cashu_wallet::cashu::nuts::Id;
use cashu_wallet::cashu::secret::Secret;
use cashu_wallet::store::MintUrlWithUnit;
use cashu_wallet::store::MintUrlWithUnitOwned;
use cashu_wallet::wallet::AmountHelper;
use cashu_wallet::wallet::CURRENCY_UNIT_SAT;
use serde::Serialize;
pub use sqlx;

use cashu_wallet::types::unixtime_ms;
use futures_util::StreamExt;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::Row;
use sqlx::SqlitePool;
use std::collections::BTreeMap as Map;
use std::num::TryFromIntError;
use std::str::FromStr;
use strum::EnumIs;

#[derive(Debug, Clone)]
pub struct LitePool {
    db: SqlitePool,
    tables: Tables,
}

impl LitePool {
    pub async fn new(db: SqlitePool, _tables: Tables) -> Result<LitePool, StoreError> {
        _tables.check()?;

        let this = Self {
            db,
            // store-sqlite/migrations
            tables: Default::default(),
        };
        this.migrate().await?;

        Ok(this)
    }

    /// https://docs.rs/sqlx-sqlite/0.7.1/sqlx_sqlite/struct.SqliteConnectOptions.html#impl-FromStr-for-SqliteConnectOptions
    pub async fn open(dbpath: &str, _tables: Tables) -> Result<LitePool, StoreError> {
        let opts = dbpath
            .parse::<SqliteConnectOptions>()?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            // prevent other thread open it
            .locking_mode(sqlx::sqlite::SqliteLockingMode::Exclusive)
            // or normal
            .synchronous(sqlx::sqlite::SqliteSynchronous::Full);

        info!("SqlitePool open: {:?}", opts);
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;

        Self::new(db, _tables).await
    }

    pub fn database(&self) -> &SqlitePool {
        &self.db
    }

    pub fn tables(&self) -> &Tables {
        &self.tables
    }

    pub async fn init(&self) -> Result<(), StoreError> {
        sqlx::migrate!("../store-sqlite/migrations")
            .run(&self.db)
            .await
            .map_err(|e| format_err!("run sqlite migrations failed: {}", e))?;

        Ok(())
    }

    #[inline]
    pub fn definition_mints<'a>(&self) -> &'static str {
        self.tables.mints
    }

    #[inline]
    pub fn definition_proofs<'a>(&self) -> &'static str {
        self.tables.proofs
    }

    #[inline]
    pub fn definition_counters<'a>(&self) -> &'static str {
        self.tables.counters
    }

    #[inline]
    pub fn definition_transactions<'a>(&self) -> &'static str {
        self.tables.transactions
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tables {
    mints: &'static str,
    proofs: &'static str,
    counters: &'static str,
    /// add records for invoices
    transactions: &'static str,
}

impl Default for Tables {
    fn default() -> Self {
        Self {
            mints: "mints",
            proofs: "proofs",
            counters: "counters",
            transactions: "transactions",
        }
    }
}

impl Tables {
    pub fn check(&self) -> anyhow::Result<()> {
        let strs = [self.mints, self.proofs, self.transactions];
        let mut names = strs.iter().filter(|s| !s.is_empty()).collect::<Vec<_>>();
        if names.len() != strs.len() {
            bail!("empty table name");
        }

        names.dedup();
        if names.len() != strs.len() {
            bail!("duplicate table name");
        }

        Ok(())
    }
}

use cashu_wallet::cashu::nuts::{nut00::Witness, nut12::ProofDleq};
use cashu_wallet::store::UnitedStore;
use cashu_wallet::wallet::{Proof, ProofExtended, ProofsExtended, Record};
use cashu_wallet::{ParseError, Url};

use cashu_wallet::types::{
    CashuTransaction, LNTransaction, Mint, Transaction, TransactionDirection, TransactionKind,
    TransactionStatus,
};

#[derive(Debug)]
//
#[derive(EnumIs, thiserror::Error)]
pub enum StoreError {
    /// Url Error
    #[error("{0}")]
    Url(#[from] ParseError),
    /// Json error
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Custom(#[from] anyhow::Error),
    #[error("{0}")]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Int(#[from] TryFromIntError),
    #[error("{0}")]
    Cashu(#[from] cashu_wallet::cashu::error::Error),
    #[error("{0}")]
    Parse(#[from] strum::ParseError),
}

impl From<StoreError> for cashu_wallet::UniError<StoreError> {
    fn from(err: StoreError) -> Self {
        Self::Store(err)
    }
}

impl From<cashu_wallet::cashu::nuts::nut01::Error> for StoreError {
    fn from(err: cashu_wallet::cashu::nuts::nut01::Error) -> Self {
        Self::Cashu(err.into())
    }
}

/// "select id, kind, amount, status, io, info, ctime, token, mint, unit, fee",
macro_rules! transaction_from_row {
    ($row: expr) => {{
        let kind = $row.get::<'_, String, _>(1).parse::<TransactionKind>()?;
        let id = $row.get::<'_, String, _>(0);
        let amount = u64::try_from($row.get::<'_, i64, _>(2))?;
        let status = $row.get::<'_, String, _>(3).parse::<TransactionStatus>()?;
        let io = $row
            .get::<'_, String, _>(4)
            .parse::<TransactionDirection>()?;
        let info = $row.get::<'_, Option<String>, _>(5);
        let time = u64::try_from($row.get::<'_, i64, _>(6))?;
        let token = $row.get::<'_, String, _>(7);
        let mint = $row.get::<'_, String, _>(8);
        let unit = $row.get::<'_, Option<String>, _>(9);

        match kind {
            TransactionKind::Cashu => {
                let tx = CashuTransaction {
                    id,
                    amount,
                    status,
                    io,
                    info,
                    time,
                    token,
                    mint,
                    unit,
                };

                tx.into()
            }
            TransactionKind::LN => {
                let tx = LNTransaction {
                    hash: id,
                    amount,
                    status,
                    io,
                    info,
                    time,
                    pr: token,
                    mint,
                    unit,
                    fee: $row
                        .get::<'_, Option<i64>, _>(10)
                        .map(|i| u64::try_from(i))
                        .transpose()?,
                };

                tx.into()
            }
        }
    }};
}

macro_rules! proof_from_row {
    ($row: expr) => {{
        let mut p = Proof {
            secret: $row
                .get::<'_, String, _>(0)
                .parse::<Secret>()
                .map_err(|e| StoreError::Cashu(e.into()))?,
            keyset_id: $row
                .get::<'_, String, _>(1)
                .parse::<Id>()
                .map_err(|e| StoreError::Cashu(e.into()))?,
            amount: u64::try_from($row.get::<'_, i64, _>(2))?.into(),
            c: PublicKey::from_str($row.get(3))?,
            dleq: None,
            witness: None,
        };

        let mint: String = $row.get(4);

        let js = $row.get::<'_, Option<String>, _>(7);
        if let Some(js) = js {
            let dleq = serde_json::from_str::<ProofDleq>(&js)?;
            p.dleq = Some(dleq);
        }

        let js = $row.get::<'_, Option<String>, _>(8);
        if let Some(js) = js {
            let dleq = serde_json::from_str::<Witness>(&js)?;
            p.witness = Some(dleq);
        }

        let p = ProofExtended {
            raw: p,
            ts: u64::try_from($row.get::<'_, i64, _>(5))?.into(),
            unit: $row.get::<'_, Option<String>, _>(6),
            js: String::new(),
        };

        (mint, p)
    }};
}

#[async_trait]
impl UnitedStore for LitePool {
    type Error = StoreError;

    // counter records
    async fn add_counter(&self, record: &Record) -> Result<(), Self::Error> {
        debug!("add_counter: {:?}", record);

        let sql = format!(
            "insert into {} (mint, keysetid, pubkey, counter, ctime) values(?, ?, ?, ?, ?)
            ON CONFLICT(mint, keysetid, pubkey) DO UPDATE SET counter = excluded.counter
            ;",
            self.definition_counters()
        );

        let counter = record.counter.to_string();
        let ts = record.ts as i64;
        sqlx::query(&sql)
            .bind(&record.mint)
            .bind(&record.keysetid)
            .bind(&record.pubkey)
            .bind(&counter)
            .bind(&ts)
            .execute(&self.db)
            .await?;

        Ok(())
    }

    async fn delete_counters(&self, mint_url: &Url) -> Result<(), Self::Error> {
        debug!("delete_counters: {}", mint_url.as_str());

        // delete can't where unit = null
        let sql = format!("delete from {} where mint = ?;", self.definition_counters());

        sqlx::query(&sql)
            .bind(mint_url.as_str())
            .execute(&self.db)
            .await?;

        Ok(())
    }

    async fn get_counters(&self, mint_url: &Url, pubkey: &str) -> Result<Vec<Record>, Self::Error> {
        let sql = format!(
        "select mint, keysetid, pubkey, counter, ctime from {} where mint=? and pubkey=? order by ctime;",
        self.definition_counters());

        let mut iter = sqlx::query(&sql)
            .bind(mint_url.as_str())
            .bind(pubkey)
            .fetch(&self.db);

        let mut ps = vec![];
        while let Some(it) = iter.next().await {
            let it = it?;

            let p = Record {
                mint: it.get(0),
                keysetid: it.get(1),
                pubkey: it.get(2),
                counter: it
                    .get::<'_, String, _>(3)
                    .parse::<u64>()
                    .map_err(|e| StoreError::Custom(e.into()))?,
                ts: u64::try_from(it.get::<'_, i64, _>(4))?,
            };

            ps.push(p);
        }

        Ok(ps)
    }
    async fn delete_proofs(
        &self,
        mint_url: &Url,
        proofs: &[ProofExtended],
    ) -> Result<(), Self::Error> {
        if proofs.is_empty() {
            return Ok(());
        }
        let mint = mint_url.as_str();

        debug!("del_proofs: {:?}", proofs);

        // delete can't where unit = null
        let sql = format!(
            "delete from {} where secret = ? and mint = ?;",
            self.definition_proofs()
        );

        let mut ctx = self.db.begin().await?;
        for p in proofs {
            sqlx::query(&sql)
                .bind(p.raw.secret.as_str())
                .bind(mint)
                // .bind(p.unit())
                .execute(ctx.as_mut())
                .await?;
        }
        ctx.commit().await?;

        Ok(())
    }
    async fn add_proofs(
        &self,
        mint_url: &Url,
        proofs: &[ProofExtended],
    ) -> Result<(), Self::Error> {
        if proofs.is_empty() {
            return Ok(());
        }
        let mint = mint_url.as_str();

        let sql = format!(
            "insert into {} (secret, keyset_id, amount, c, mint, ctime, unit, dleq, witness) values(?, ?, ?, ?, ?, ?, ?, ?, ?);",
            self.definition_proofs()
        );

        debug!("add_proofs: {:?}", proofs);
        let mut ctx = self.db.begin().await?;
        for p in proofs {
            let c = p.raw.c.to_string();
            let ts: i64 = p.ts.unwrap_or_else(unixtime_ms).try_into()?;
            let amount: i64 = p.raw.amount.to_u64().try_into()?;

            let mut dleq = None;
            if let Some(w) = &p.raw.dleq {
                let js = serde_json::to_string(&w)?;
                dleq = Some(js);
            }

            let mut witness = None;
            if let Some(w) = &p.raw.witness {
                let js = serde_json::to_string(&w)?;
                witness = Some(js);
            }

            sqlx::query(&sql)
                .bind(p.raw.secret.as_str())
                .bind(&p.raw.keyset_id.to_string())
                .bind(amount)
                .bind(&c)
                .bind(mint)
                .bind(ts)
                .bind(p.unit())
                .bind(dleq)
                .bind(witness)
                .execute(ctx.as_mut())
                .await?;
        }
        ctx.commit().await?;

        Ok(())
    }
    async fn get_proofs_limit_unit(
        &self,
        mint_url: &Url,
        unit: &str,
    ) -> Result<ProofsExtended, Self::Error> {
        // debug!("get.proofs.len: {:?}", table.len());
        let mint = mint_url.as_str();

        let sql = if unit == CURRENCY_UNIT_SAT {
            format!(
            "select secret, keyset_id, amount, c, mint, ctime, unit, dleq, witness from {} where mint=? and (unit=? or unit is null) order by ctime;",
            self.definition_proofs()
        )
        } else {
            format!(
                "select secret, keyset_id, amount, c, mint, ctime, unit, dleq, witness from {} where mint=? and unit =? order by ctime;",
                self.definition_proofs()
            )
        };

        let mut iter = sqlx::query(&sql).bind(mint).bind(unit).fetch(&self.db);

        let mut proofs = vec![];

        while let Some(it) = iter.next().await {
            let it = it?;
            let (_mint, p) = proof_from_row!(it);
            proofs.push(p);
        }

        Ok(proofs)
    }
    async fn get_proofs(&self, mint_url: &Url) -> Result<Map<String, ProofsExtended>, Self::Error> {
        // debug!("get.proofs.len: {:?}", table.len());
        let mint = mint_url.as_str();

        let sql = format!(
            "select secret, keyset_id, amount, c, mint, ctime, unit, dleq, witness from {} where mint=? order by ctime;",
            self.definition_proofs()
        );

        let mut iter = sqlx::query(&sql).bind(mint).fetch(&self.db);

        let mut proofs = Map::new();

        while let Some(it) = iter.next().await {
            let it = it?;
            let (_mint, p) = proof_from_row!(it);
            let k = p.unit().unwrap_or(CURRENCY_UNIT_SAT);
            if !proofs.contains_key(k) {
                proofs.insert(k.to_owned(), vec![]);
            }
            let ps: &mut Vec<_> = proofs.get_mut(k).unwrap();
            ps.push(p);
        }

        Ok(proofs)
    }
    async fn get_all_proofs(
        &self,
    ) -> Result<Map<MintUrlWithUnitOwned, ProofsExtended>, Self::Error> {
        // debug!("get.proofs.len: {:?}", table.len());

        let sql = format!(
            "select secret, keyset_id, amount, c, mint, ctime, unit, dleq, witness from {} order by ctime;",
            self.definition_proofs()
        );

        let mut iter = sqlx::query(&sql).fetch(&self.db);

        let mut proofs = Map::new();

        while let Some(it) = iter.next().await {
            let it = it?;
            let (mint, p) = proof_from_row!(it);

            let key = p.unit().unwrap_or(CURRENCY_UNIT_SAT);
            let key = MintUrlWithUnit::new(mint, key).into_owned();

            let ps = proofs.entry(key).or_insert(vec![]);
            ps.push(p);
        }

        Ok(proofs)
    }
    /// try open tables
    async fn migrate(&self) -> Result<(), Self::Error> {
        self.init().await?;
        Ok(())
    }
    //
    // mints
    /// overwrite it
    async fn add_mint(&self, mint: &Mint) -> Result<(), Self::Error> {
        let mut mi = None;
        if let Some(m) = &mint.info {
            let js = serde_json::to_string(&m)?;
            mi = Some(js);
        }
        let sql = format!(
            "insert into {} (url, active, info, ctime) values(?, ?, ?, ?)
            ON CONFLICT(url) DO UPDATE SET active = excluded.active, info=excluded.info
            ;",
            self.definition_mints()
        );

        let ts = mint.time as i64;
        sqlx::query(&sql)
            .bind(&mint.url)
            .bind(mint.active)
            .bind(&mi)
            .bind(ts)
            .execute(&self.db)
            .await?;

        Ok(())
    }
    async fn get_mint(&self, mint_url: &str) -> Result<Option<Mint>, Self::Error> {
        let sql = format!(
            "select url, active, ctime, info from {} where url=?",
            self.definition_mints()
        );

        let mint = sqlx::query(&sql)
            .bind(mint_url)
            .fetch_optional(&self.db)
            .await?;
        if mint.is_none() {
            return Ok(None);
        }
        let it = mint.unwrap();
        let info = it.get::<'_, Option<String>, _>(3);
        let info =
            info.and_then(|i| serde_json::from_str::<cashu_wallet::types::MintInfo>(&i).ok());

        let mint = Mint {
            url: it.get(0),
            active: it.get(1),
            time: u64::try_from(it.get::<'_, i64, _>(2))?,
            info,
        };

        Ok(Some(mint))
    }
    async fn get_mints(&self) -> Result<Vec<Mint>, Self::Error> {
        let sql = format!(
            "select url, active, ctime, info from {} order by url",
            self.definition_mints()
        );

        let rows = sqlx::query(&sql).fetch_all(&self.db).await?;

        let mut mints = vec![];
        for it in rows {
            let info = it.get::<'_, Option<String>, _>(3);
            let info =
                info.and_then(|i| serde_json::from_str::<cashu_wallet::types::MintInfo>(&i).ok());

            let mint = Mint {
                url: it.get(0),
                active: it.get(1),
                time: u64::try_from(it.get::<'_, i64, _>(2))?,
                info,
            };
            mints.push(mint);
        }

        Ok(mints)
    }
    //
    // tx
    async fn add_transaction(&self, tx: &Transaction) -> Result<(), Self::Error> {
        let id = tx.id();

        let sql = format!(
            "insert into {} (id, kind, amount, status, io, info, ctime, token, mint, unit, fee) values(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id, io) DO UPDATE SET status = excluded.status, info=excluded.info, fee=excluded.fee
            ;",
            self.definition_transactions()
        );

        debug!(
            "add_transaction.sql: {} {} {}",
            id,
            tx.status(),
            tx.direction()
        );

        let ts = tx.time() as i64;

        let mut ctx = self.db.begin().await?;
        sqlx::query(&sql)
            .bind(&id)
            .bind(tx.kind().as_ref())
            .bind(i64::try_from(tx.amount())?)
            .bind(tx.status().as_ref())
            .bind(tx.direction().as_ref())
            .bind(tx.info())
            .bind(ts)
            .bind(tx.content())
            .bind(tx.mint_url())
            .bind(tx.unit())
            .bind(tx.fee().map(|f| i64::try_from(f)).transpose()?)
            .execute(ctx.as_mut())
            .await?;
        ctx.commit().await?;

        Ok(())
    }

    async fn get_transaction(&self, txid: &str) -> Result<Option<Transaction>, Self::Error> {
        let sql = format!(
            "select id, kind, amount, status, io, info, ctime, token, mint, unit, fee from {} where id=?;",
            self.definition_transactions()
        );

        let row = sqlx::query(&sql)
            .bind(txid)
            .fetch_optional(&self.db)
            .await?;
        if row.is_none() {
            return Ok(None);
        }
        let row = row.unwrap();

        let tx = transaction_from_row!(row);

        Ok(Some(tx))
    }

    async fn get_transactions(
        &self,
        status: &[TransactionStatus],
    ) -> Result<Vec<Transaction>, Self::Error> {
        // https://github.com/launchbadge/sqlx/issues/656
        let status_slice = status
            .iter()
            .map(|s| format!("'{}'", s.as_ref()))
            .collect::<Vec<_>>();
        let status_array = status_slice.join(",");

        let sql = format!(
            "select id, kind, amount, status, io, info, ctime, token, mint, unit, fee from {} where status in ({}) order by ctime;",
            self.definition_transactions(),
            status_array
        );

        let mut rows = sqlx::query(&sql).fetch(&self.db);

        let mut txs = vec![];
        while let Some(it) = rows.next().await {
            let it = it?;
            let tx = transaction_from_row!(it);
            txs.push(tx);
        }

        Ok(txs)
    }

    async fn get_transactions_with_offset(
        &self,
        offset: usize,
        limit: usize,
        kinds: &[TransactionKind],
    ) -> Result<Vec<Transaction>, Self::Error> {
        // https://github.com/launchbadge/sqlx/issues/656
        let ks_slice = kinds
            .iter()
            .map(|s| format!("'{}'", s.as_ref()))
            .collect::<Vec<_>>();
        let ks_array = ks_slice.join(",");

        let sql = format!(
            "select id, kind, amount, status, io, info, ctime, token, mint, unit, fee from {} where kind in ({}) order by ctime desc limit ? offset ?;",
            self.definition_transactions(), &ks_array
        );

        let mut rows = sqlx::query(&sql)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch(&self.db);

        let mut txs = vec![];
        while let Some(it) = rows.next().await {
            let it = it?;
            let tx = transaction_from_row!(it);
            txs.push(tx);
        }

        Ok(txs)
    }

    async fn delete_transactions(
        &self,
        status: &[TransactionStatus],
        unix_timestamp_ms_le: u64,
    ) -> Result<u64, Self::Error> {
        let status_slice = status
            .iter()
            .map(|s| format!("'{}'", s.as_ref()))
            .collect::<Vec<_>>();
        let status_array = status_slice.join(",");

        let sql = format!(
            "delete from {} where ctime<=? and status in ({});",
            self.definition_transactions(),
            status_array
        );

        let row = sqlx::query(&sql)
            .bind(unix_timestamp_ms_le as i64)
            // .bind(&status_array)
            .execute(&self.db)
            .await?;

        Ok(row.rows_affected())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    // cargo test store::impl_redb --  --nocapture
    #[tokio::test]
    async fn it_works_mint() {
        // let (_td, tf) = cashu_wallet::store::tests::tmpfi("test.sqlite");
        let tf = "sqlite::memory:";

        let db = LitePool::open(tf, Default::default()).await.unwrap();
        // let w = UnitedWallet::new(db, c);
        cashu_wallet::store::tests::test_mint(&db).await.unwrap();
    }

    #[tokio::test]
    async fn it_works_counter() {
        let tf = "sqlite::memory:";

        let db = LitePool::open(tf, Default::default()).await.unwrap();
        // let w = UnitedWallet::new(db, c);
        cashu_wallet::store::tests::test_counter(&db).await.unwrap();
    }

    #[tokio::test]
    async fn it_works_proof() {
        let tf = "sqlite::memory:";

        let db = LitePool::open(tf, Default::default()).await.unwrap();
        cashu_wallet::store::tests::test_proof(&db, Some(true))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn it_works_transaction_cashu() {
        let tf = "sqlite::memory:";

        let db = LitePool::open(tf, Default::default()).await.unwrap();
        cashu_wallet::store::tests::test_transaction_cashu(&db)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn it_works_transaction_ln() {
        let tf = "sqlite::memory:";

        let db = LitePool::open(tf, Default::default()).await.unwrap();
        cashu_wallet::store::tests::test_transaction_ln(&db)
            .await
            .unwrap();
    }
}
