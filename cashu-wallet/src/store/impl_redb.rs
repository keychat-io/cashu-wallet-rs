pub use redb;

#[allow(unused_imports)]
use redb::{
    Database, MultimapTableHandle, ReadableMultimapTable, ReadableTable, ReadableTableMetadata,
    TableHandle,
};
use std::collections::BTreeMap as Map;
use std::sync::Arc;
use strum::EnumIs;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tables {
    pub mints: &'static str,
    // pub keysets: &'static str,
    pub proofs: &'static str,
    pub counters: &'static str,
    /// add records for invoices
    pub transactions: &'static str,
    pub pending_transactions: &'static str,
}

impl Default for Tables {
    fn default() -> Self {
        Self {
            mints: "mints",
            // keysets: "keysets",
            proofs: "proofs",
            counters: "counters",
            transactions: "transactions",
            pending_transactions: "pending_transactions",
        }
    }
}

impl Tables {
    pub fn check(&self) -> anyhow::Result<()> {
        let strs = [
            self.mints,
            // self.keysets,
            self.proofs,
            self.counters,
            self.transactions,
            self.pending_transactions,
        ];
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

/// redb wrap
pub struct Redb {
    tables: Tables,
    db: Database,
}

impl Redb {
    pub fn new(db: Database, tables: Tables) -> Result<Arc<Redb>, StoreError> {
        tables.check()?;

        let this = Self { db, tables };
        this.init()?;

        Ok(Arc::new(this))
    }

    pub fn open<P: AsRef<std::path::Path>>(
        dbpath: P,
        tables: Tables,
    ) -> Result<Arc<Redb>, StoreError> {
        let db = Database::builder().create(dbpath)?;

        Self::new(db, tables)
    }

    pub fn database(&self) -> &Database {
        &self.db
    }

    pub fn tables(&self) -> &Tables {
        &self.tables
    }

    pub fn init(&self) -> Result<(), StoreError> {
        let tn = self.db.begin_write()?;
        {
            tn.open_table(self.definition_mints())?;
            tn.open_multimap_table(self.definition_proofs())?;
            tn.open_multimap_table(self.definition_counters())?;
            tn.open_table(self.definition_transactions())?;
            tn.open_table(self.definition_pending_transactions())?;
        }
        tn.commit()?;

        Ok(())
    }

    // <'a>: not use the self life
    ///
    /// <mint, info>
    #[inline]
    pub fn definition_mints<'a>(&self) -> TableDefinition<'static, &'a str, &'a str> {
        TableDefinition::new(self.tables.mints)
    }

    /// <mint, proofJSON..>
    #[inline]
    pub fn definition_proofs<'a>(&self) -> MultimapTableDefinition<'static, &'a str, &'a str> {
        MultimapTableDefinition::new(self.tables.proofs)
    }
    /// <mint, CounterRecordJSON..>
    #[inline]
    pub fn definition_counters<'a>(&self) -> MultimapTableDefinition<'static, &'a str, &'a str> {
        MultimapTableDefinition::new(&self.tables.counters)
    }
    /// add direct(i/o) to transaction
    /// <txid/txidIn, TxJson>
    ///
    /// txidIn for save send to self
    #[inline]
    pub fn definition_transactions<'a>(&self) -> TableDefinition<'static, &'a str, &'a str> {
        TableDefinition::new(self.tables.transactions)
    }
    #[inline]
    pub fn definition_pending_transactions<'a>(
        &self,
    ) -> TableDefinition<'static, &'a str, &'a str> {
        TableDefinition::new(self.tables.pending_transactions)
    }
}

use crate::store::cmp_by_asc;
use crate::store::UnitedStore;
use crate::store::{MintUrlWithUnit, MintUrlWithUnitOwned};
use redb::{MultimapTableDefinition, TableDefinition};

use crate::types::{Mint, Transaction, TransactionDirection, TransactionStatus};

use crate::wallet::{MintUrl as Url, ProofExtended, ProofsExtended, Record, CURRENCY_UNIT_SAT};

#[derive(Debug)]
//
#[derive(EnumIs, thiserror::Error)]
pub enum StoreError {
    /// Url Error
    #[error("{0}")]
    Url(#[from] url::ParseError),
    /// Json error
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    // #[error("{0}")]
    // Db(#[from] redb::Error),
    #[error("{0}")]
    Database(#[from] redb::DatabaseError),
    #[error("{0}")]
    Commit(#[from] redb::CommitError),
    #[error("{0}")]
    Store(#[from] redb::StorageError),
    #[error("{0}")]
    Table(anyhow::Error),
    // Table(#[from] redb::TableError),
    #[error("{0}")]
    Transaction(#[from] redb::TransactionError),
    #[error("{0}")]
    Custom(#[from] anyhow::Error),
}

impl From<redb::TableError> for StoreError {
    fn from(err: redb::TableError) -> Self {
        Self::Table(err.into())
    }
}

impl From<StoreError> for crate::unity::Error<StoreError> {
    fn from(err: StoreError) -> Self {
        Self::Store(err)
    }
}

#[async_trait]
impl UnitedStore for Redb {
    type Error = StoreError;
    // counter records
    async fn add_counter(&self, record: &Record) -> Result<(), Self::Error> {
        let json = serde_json::to_string(record)?;
        debug!("add_counter: {:?}", json);

        let define = self.definition_counters();

        let tn = self.database().begin_write()?;
        {
            let mut table = tn.open_multimap_table(define)?;

            let replace = table.insert(record.mint.as_str(), json.as_str())?;

            let mut olds = vec![];
            if !replace {
                let kvs = table.get(record.mint.as_str())?;
                for p in kvs {
                    let js = p?;
                    let t: Record = serde_json::from_str(js.value())?;

                    if t.pubkey == record.pubkey
                        && t.keysetid == record.keysetid
                        && t.counter < record.counter
                    {
                        olds.push(js.value().to_owned());
                    }
                }
            }

            debug!("add_counter: delete {:?}", olds);
            for r in olds {
                table.remove(record.mint.as_str(), r.as_str())?;
            }
        }
        tn.commit()?;
        Ok(())
    }
    async fn delete_counters(&self, mint_url: &Url) -> Result<(), Self::Error> {
        let mint = mint_url.as_str();
        debug!("delete_counters: {}", mint);

        let define = self.definition_counters();

        let tn = self.database().begin_write()?;
        let mut table = tn.open_multimap_table(define)?;
        let vals = table.remove_all(mint)?;

        debug!("delete_counters ok: {} {}", mint, vals.count());

        Ok(())
    }
    async fn get_counters(&self, mint_url: &Url, pubkey: &str) -> Result<Vec<Record>, Self::Error> {
        let define = self.definition_counters();

        let tn = self.database().begin_read()?;
        let table = tn.open_multimap_table(define)?;

        #[rustfmt::skip]
        debug!("get_counters {} {} table.len: {:?}", mint_url.as_str(), pubkey, table.len());

        let kvs = table.get(mint_url.as_str())?;

        let mut proofs = Vec::new();
        for kv in kvs.flatten() {
            let json = kv.value();
            debug!("get.counters: {}", json);

            let p: Record = serde_json::from_str(json)?;

            if p.pubkey == pubkey {
                proofs.push(p);
            }
        }

        proofs.sort_by(|a, b| cmp_by_asc(a.ts, b.ts));

        #[rustfmt::skip]
        debug!("get_counters {} {} get: {:?}", mint_url.as_str(), pubkey, proofs);

        Ok(proofs)
    }

    async fn delete_proofs(
        &self,
        mint_url: &Url,
        proofs: &[ProofExtended],
    ) -> Result<(), Self::Error> {
        if proofs.is_empty() {
            return Ok(());
        }

        let mut ps: Vec<std::borrow::Cow<'_, str>> = Vec::with_capacity(proofs.len());
        for p in proofs {
            if p.js.is_empty() {
                let json = serde_json::to_string(&p)?;
                ps.push(json.into());
            } else {
                ps.push(p.js.as_str().into());
            }
        }

        debug!("del_proofs: {:?}", ps);

        let define = self.definition_proofs();

        let tn = self.database().begin_write()?;
        {
            let mut table = tn.open_multimap_table(define)?;

            debug!("del0.proofs.len: {:?}", table.len());
            for p in &ps {
                table.remove(mint_url.as_str(), p.as_ref())?;
            }
            debug!("del1.proofs.len: {:?}", table.len());
        }
        tn.commit()?;

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

        let mut ps: Vec<std::borrow::Cow<'_, str>> = Vec::with_capacity(proofs.len());
        for p in proofs {
            if p.js.is_empty() {
                let json = serde_json::to_string(&p)?;
                ps.push(json.into());
            } else {
                ps.push(p.js.as_str().into());
            }
        }

        debug!("add_proofs: {:?}", ps);

        let define = self.definition_proofs();

        let tn = self.database().begin_write()?;
        {
            let mut table = tn.open_multimap_table(define)?;

            debug!("add.proofs.len0: {:?}", table.len());
            for p in &ps {
                table.insert(mint_url.as_str(), p.as_ref())?;
            }
            debug!("add.proofs.len1: {:?}", table.len());
        }
        tn.commit()?;

        Ok(())
    }
    async fn get_proofs_limit_unit(
        &self,
        mint_url: &Url,
        unit: &str,
    ) -> Result<ProofsExtended, Self::Error> {
        let define = self.definition_proofs();

        let tn = self.database().begin_read()?;
        let table = tn.open_multimap_table(define)?;
        debug!("get.proofs.len: {:?}", table.len());

        let kvs = table.get(mint_url.as_str())?;

        let mut proofs = vec![];
        for kv in kvs.flatten() {
            let json = kv.value();
            debug!("get.proofs: {}", json);

            let p: ProofExtended = serde_json::from_str(json)?;
            let k = p.unit().unwrap_or(CURRENCY_UNIT_SAT);
            if k == unit {
                proofs.push(p.json(json.to_owned()));
            }
        }

        proofs.sort_by(|a, b| cmp_by_asc(a.ts, b.ts));

        Ok(proofs)
    }
    async fn get_proofs(&self, mint_url: &Url) -> Result<Map<String, ProofsExtended>, Self::Error> {
        let define = self.definition_proofs();

        let tn = self.database().begin_read()?;
        let table = tn.open_multimap_table(define)?;
        debug!("get.proofs.len: {:?}", table.len());

        let kvs = table.get(mint_url.as_str())?;

        let mut proofs = Map::new();
        for kv in kvs.flatten() {
            let json = kv.value();
            debug!("get.proofs: {}", json);

            let p: ProofExtended = serde_json::from_str(json)?;

            let k = p.unit().unwrap_or(CURRENCY_UNIT_SAT);
            if !proofs.contains_key(k) {
                proofs.insert(k.to_owned(), vec![]);
            }
            let ps: &mut Vec<_> = proofs.get_mut(k).unwrap();
            ps.push(p.json(json.to_owned()));
        }

        Ok(proofs
            .into_iter()
            .map(|(k, mut v)| {
                v.sort_by(|a, b| cmp_by_asc(a.ts, b.ts));
                (k.to_owned(), v)
            })
            .collect())
    }
    async fn get_all_proofs(
        &self,
    ) -> Result<Map<MintUrlWithUnitOwned, ProofsExtended>, Self::Error> {
        let define = self.definition_proofs();

        let mut map = Map::new();

        let tn = self.database().begin_read()?;
        let table = tn.open_multimap_table(define)?;
        debug!("get.proofs.len: {:?}", table.len());

        for kvs in table.iter()? {
            let kvs = kvs?;

            let mut proofs = Map::new();
            for kv in kvs.1.flatten() {
                let json = kv.value();
                debug!("get.proofs: {}", json);

                let p: ProofExtended = serde_json::from_str(json)?;
                let k = p.unit().unwrap_or(CURRENCY_UNIT_SAT);
                if !proofs.contains_key(k) {
                    proofs.insert(k.to_owned(), vec![]);
                }
                let ps: &mut Vec<_> = proofs.get_mut(k).unwrap();
                ps.push(p.json(json.to_owned()));
            }

            for (k, mut ps) in proofs.into_iter() {
                ps.sort_by(|a, b| cmp_by_asc(a.ts, b.ts));
                map.insert(MintUrlWithUnit::new(kvs.0.value(), k).into_owned(), ps);
            }
        }

        Ok(map)
    }
    /// try open tables
    async fn migrate(&self) -> Result<(), Self::Error> {
        self.init()?;
        Ok(())
    }
    //
    // mints
    /// overwrite it
    async fn add_mint(&self, mint: &Mint) -> Result<(), Self::Error> {
        let json = serde_json::to_string(mint)?;
        let url: Url = mint.url.parse()?;

        let define = self.definition_mints();

        let tn = self.database().begin_write()?;
        {
            let mut table = tn.open_table(define)?;
            table.insert(url.as_str(), json.as_str())?;
        }
        tn.commit()?;

        Ok(())
    }
    async fn get_mint(&self, mint_url: &str) -> Result<Option<Mint>, Self::Error> {
        let define = self.definition_mints();

        let tn = self.database().begin_read()?;
        {
            let table = tn.open_table(define)?;
            let got = table.get(mint_url)?;

            if got.is_none() {
                return Ok(None);
            }

            let json = got.unwrap();
            let js: Mint = serde_json::from_str(json.value())?;
            Ok(Some(js))
        }
    }
    async fn get_mints(&self) -> Result<Vec<Mint>, Self::Error> {
        let define = self.definition_mints();

        let tn = self.database().begin_read()?;
        {
            let table = tn.open_table(define)?;
            let mut mints = Vec::with_capacity(table.len()? as usize);

            for row in table.iter()? {
                let json = row?;
                let js: Mint = serde_json::from_str(json.1.value())?;
                mints.push(js);
            }
            mints.sort_by(|a, b| cmp_by_asc(&a.url, &b.url));
            Ok(mints)
        }
    }
    //
    // tx
    async fn add_transaction(&self, tx: &Transaction) -> Result<(), Self::Error> {
        let txid = tx.id();
        let json = serde_json::to_string(tx)?;

        debug!("add_transaction: {}", json);

        let define = self.definition_transactions();
        let define_pending = self.definition_pending_transactions();

        let tn = self.database().begin_write()?;
        {
            let mut table_pending = tn.open_table(define_pending)?;

            if tx.is_pending() {
                table_pending.insert(txid, json.as_str())?;
            } else {
                let record = table_pending.remove(txid)?;

                let mut table = tn.open_table(define)?;
                table.insert(txid, json.as_str())?;

                // add a record for send to self
                if tx.status() == TransactionStatus::Success
                    && tx.direction() == TransactionDirection::In
                {
                    if let Some(old) = record {
                        if let Ok(mut oldtx) = serde_json::from_str::<Transaction>(old.value()) {
                            if oldtx.direction() == TransactionDirection::Out {
                                *oldtx.status_mut() = TransactionStatus::Success;

                                let txid_in = format!("{}{}", txid, oldtx.direction().as_ref());
                                let json = serde_json::to_string(&oldtx)?;
                                table.insert(txid_in.as_str(), json.as_str())?;
                            }
                        }
                    }
                }
            }
        }
        tn.commit()?;

        Ok(())
    }
    async fn get_transaction(&self, txid: &str) -> Result<Option<Transaction>, Self::Error> {
        let define = self.definition_transactions();
        let define_pending = self.definition_pending_transactions();

        let tn = self.database().begin_read()?;
        macro_rules! f {
            ($name: expr) => {
                let table = tn.open_table($name)?;
                let json = table.get(txid)?;
                if let Some(json) = json {
                    let js: Transaction = serde_json::from_str(&json.value())?;
                    return Ok(Some(js));
                }
            };
        }

        f!(define_pending);
        f!(define);

        Ok(None)
    }

    async fn get_transactions(
        &self,
        status: &[TransactionStatus],
    ) -> Result<Vec<Transaction>, Self::Error> {
        let define = self.definition_transactions();
        let define_pending = self.definition_pending_transactions();

        let pendingc = status.iter().filter(|s| s.is_pending()).count();
        let some_is_pending = pendingc > 0;
        let some_not_pending = pendingc < status.len();

        let mut txs = vec![];

        let tn = self.database().begin_read()?;
        macro_rules! f {
            ($table: expr) => {
                let table = tn.open_table($table)?;

                debug!(
                    "get_transactions {}/{}: {:?}, {}.len: {:?}",
                    pendingc,
                    status.len(),
                    status,
                    $table.name(),
                    table.len()
                );

                for row in table.iter()? {
                    let json = row?;
                    let js: Transaction = serde_json::from_str(&json.1.value())?;

                    debug!(
                        "get_transactions: {:?}, {}: {:?}",
                        status,
                        $table.name(),
                        json.1.value(),
                    );

                    if status.contains(&js.status()) {
                        txs.push(js);
                    }
                }
            };
        }

        if some_is_pending {
            f!(define_pending);
        }

        if some_not_pending {
            f!(define);
        }

        txs.sort_by(|a, b| cmp_by_asc(a.time(), b.time()));

        Ok(txs)
    }
    async fn delete_transactions(
        &self,
        status: &[TransactionStatus],
        unix_timestamp_ms_le: u64,
    ) -> Result<u64, Self::Error> {
        let define = self.definition_transactions();
        let define_pending = self.definition_pending_transactions();

        let pendingc = status.iter().filter(|s| s.is_pending()).count();
        let some_is_pending = pendingc > 0;
        let some_not_pending = pendingc < status.len();

        let mut count = 0u64;
        let tn = self.database().begin_write()?;

        macro_rules! f {
            ($table: expr) => {
                let mut transaction_table = tn.open_table($table)?;
                count += transaction_table
                    .extract_if(|_k, v| {
                        let is = serde_json::from_str::<Transaction>(v).map(|js| {
                            js.time() <= unix_timestamp_ms_le && status.contains(&js.status())
                        });

                        let remove = is.as_ref().ok().cloned().unwrap_or(false);
                        debug!(
                            "drain_filter.{} {} {:?}: {} {:?}->{}",
                            $table.name(),
                            unix_timestamp_ms_le,
                            status,
                            _k,
                            is,
                            remove,
                        );

                        remove
                    })?
                    .count() as u64;
            };
        }

        if some_is_pending {
            f!(define_pending);
        }

        if some_not_pending {
            f!(define);
        }

        tn.commit()?;

        Ok(count)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    // cargo test store::impl_redb --  --nocapture
    #[tokio::test]
    async fn it_works_mint() {
        let (_td, tf) = crate::store::tests::tmpfi("test.redb");

        let db = Redb::open(tf, Default::default()).unwrap();
        crate::store::tests::test_mint(&db).await.unwrap();
    }

    #[tokio::test]
    async fn it_works_counter() {
        let (_td, tf) = crate::store::tests::tmpfi("test.redb");

        let db = Redb::open(tf, Default::default()).unwrap();
        crate::store::tests::test_counter(&db).await.unwrap();
    }

    #[tokio::test]
    async fn it_works_proof() {
        let (_td, tf) = crate::store::tests::tmpfi("test.redb");

        let db = Redb::open(tf, Default::default()).unwrap();
        crate::store::tests::test_proof(&db, None).await.unwrap();
    }

    #[tokio::test]
    async fn it_works_transaction_cashu() {
        let (_td, tf) = crate::store::tests::tmpfi("test.redb");

        let db = Redb::open(tf, Default::default()).unwrap();
        crate::store::tests::test_transaction_cashu(&db)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn it_works_transaction_ln() {
        let (_td, tf) = crate::store::tests::tmpfi("test.redb");

        let db = Redb::open(tf, Default::default()).unwrap();
        crate::store::tests::test_transaction_ln(&db).await.unwrap();
    }
}
