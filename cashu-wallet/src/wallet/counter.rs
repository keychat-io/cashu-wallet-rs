use super::MintUrl as Url;
pub use bip39::Mnemonic;
use cashu::nuts::nut02::Id as KeySetId;
use cashu::nuts::nut02::KeySet;
use cashu::nuts::BlindedMessage;
use cashu::nuts::PreMint;
use cashu::Amount;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub mint: String,
    // pubkey for mnemonic
    pub keysetid: String,
    pub counter: u64,
    pub ts: u64,
    pub pubkey: String,
}

impl Record {
    pub(crate) fn new(mint: &str, keysetid: String, pubkey: Option<String>) -> Self {
        Self {
            mint: mint.to_owned(),
            pubkey: pubkey.unwrap_or_default(),
            keysetid: keysetid.to_owned(),
            ts: unixtime_ms(),
            counter: 0,
        }
    }
    pub(crate) fn to_counter(self, keysets: &[KeySet]) -> Option<Counter> {
        let ksid: KeySetId = self.keysetid.parse().ok()?;
        let ksidx = keysets.iter().position(|k| k.id == ksid)?;

        Some(Counter {
            state: self.counter,
            record: self,
            keysetidx: ksidx,
        })
    }
}

use crate::types::unixtime_ms;
use std::convert::Infallible;
use std::sync::Arc;

use super::CURRENCY_UNIT_SAT;

#[derive(Debug, Default)]
pub struct Counter {
    pub(crate) record: Record,
    state: u64,
    // keyset index
    pub(crate) keysetidx: usize,
}

impl Counter {
    fn next(&mut self) -> u64 {
        let it = self.state;
        self.state += 1;
        it
    }
    pub fn keyset<'s, 'l: 's>(&'s self, keysets: &'l [KeySet]) -> &'l KeySet {
        &keysets[self.keysetidx]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MnemonicInfo {
    mnemonic: Mnemonic,
    pubkey: String,
}

impl MnemonicInfo {
    pub fn new(mnemonic: Mnemonic) -> anyhow::Result<Self> {
        let pubkey = get_ident_pubkey(&mnemonic)?;
        Ok(Self { mnemonic, pubkey })
    }
    pub fn with_words(words: &str) -> anyhow::Result<Self> {
        let mnemonic = words.parse()?;
        Self::new(mnemonic)
    }
    pub fn generate(words: usize) -> anyhow::Result<Self> {
        let mnemonic = Mnemonic::generate(words)?;
        Self::new(mnemonic)
    }
    pub fn pubkey(&self) -> &str {
        &self.pubkey
    }
    pub fn mnemonic(&self) -> &Mnemonic {
        &self.mnemonic
    }
}

/// m / 129372' / 0' / keyset_k_int' / counter' / secret||r
/// m / 129372' / 0'
fn get_ident_pubkey(mnemonic: &Mnemonic) -> anyhow::Result<String> {
    use bitcoin::bip32::{DerivationPath, ExtendedPrivKey};
    use bitcoin::Network;
    use cashu::SECP256K1;

    let path: DerivationPath = "m/129372'/0'".parse().unwrap();

    let seed: [u8; 64] = mnemonic.to_seed("");
    let bip32_root_key = ExtendedPrivKey::new_master(Network::Bitcoin, &seed)?;
    let derived_xpriv = bip32_root_key.derive_priv(&SECP256K1, &path)?;
    let ident = derived_xpriv
        .to_keypair(&SECP256K1)
        .public_key()
        .to_string();
    Ok(ident)
}

use super::Error;
use tokio::sync::Mutex;
use tokio::sync::OwnedMutexGuard as MutexGuard;
#[derive(Debug, Default, Clone)]
pub struct ManagerBox {
    pub(super) manager: Option<Arc<Mutex<Manager>>>,
}
impl ManagerBox {
    pub fn new(manager: Option<Arc<Mutex<Manager>>>) -> Self {
        Self { manager }
    }
    pub async fn keyset0<'s, 'l: 's>(
        &'s self,
        unit: Option<&str>,
        keysets: &'l [KeySet],
    ) -> Result<&'l KeySet, Error> {
        let unit = unit.unwrap_or(CURRENCY_UNIT_SAT);

        let ks = if self.manager.is_none() {
            keysets.iter().find(|k| k.unit.as_str() == unit)
        } else {
            let lock = self.manager.as_ref().unwrap().lock().await;
            lock.counters
                .iter()
                .map(|c| c.keyset(keysets))
                .find(|k| k.unit.as_str() == unit)
        }
        .ok_or_else(|| Error::Custom(format_err!("counters not find suitable keyset")))?;

        Ok(ks)
    }
    pub async fn maybe_lock<'s>(&'s self) -> ManagerGuard {
        let mut guard = None;
        if let Some(lock) = self.manager.as_ref() {
            let l = lock.clone().lock_owned().await;
            guard = Some(l);
        }
        ManagerGuard {
            guard,
            counter: None,
        }
    }
}

pub struct ManagerGuard {
    pub(super) guard: Option<MutexGuard<Manager>>,
    counter: Option<Counter>,
}
impl ManagerGuard {
    pub fn mnemonic(&self) -> Option<Arc<MnemonicInfo>> {
        self.guard.as_deref().and_then(|m| m.mnemonic.clone())
    }
    pub fn start_count<'s, 'l: 's>(
        &'s mut self,
        unit: Option<&'l str>,
        keysets: &'l [KeySet],
    ) -> anyhow::Result<ManagerCounter<'s>> {
        if let Some(g) = self.guard.as_mut() {
            g.start_count(unit, keysets)
        } else {
            let unit = unit.unwrap_or(CURRENCY_UNIT_SAT);
            if self.counter.is_none() {
                let ks = keysets
                    .iter()
                    .find(|k| k.unit.as_str() == unit)
                    .ok_or_else(|| format_err!("counters not find suitable keyset"))?;
                let c = Record::new("nocommit", ks.id.to_string(), None)
                    .to_counter(keysets)
                    .expect("counters not find suitable keyset2");
                self.counter = Some(c);
            }
            let counter = self.counter.as_mut().unwrap();

            let mc = ManagerCounter {
                keyset: counter.keyset(keysets),
                mnemonic: None,
                counter,
            };

            Ok(mc)
        }
    }
}

#[derive(Debug, Default)]
pub struct Manager {
    pub(crate) mint_url: String,
    pub(crate) mnemonic: Option<Arc<MnemonicInfo>>,
    // new counter's key is empty
    pub(crate) counters: Vec<Counter>,
}

impl Manager {
    pub fn new(mint_url: &Url) -> Self {
        Self {
            mint_url: mint_url.as_str().to_owned(),
            counters: Default::default(),
            mnemonic: None,
        }
    }

    pub fn mnemonic(mut self, mnemonic: Option<Arc<MnemonicInfo>>) -> Self {
        self.mnemonic = mnemonic;
        self
    }

    pub fn records(mut self, records: Vec<Record>, keysets: &[KeySet]) -> Self {
        // println!("{:?}", keysets.len());
        let mut counters = records
            .into_iter()
            .filter_map(|r| r.to_counter(keysets))
            .collect::<Vec<_>>();

        // push new keysets to tail
        let news = keysets
            .iter()
            .filter(|ks| !counters.iter().any(|c| c.keyset(keysets).id == ks.id))
            .collect::<Vec<_>>();

        for ks in news {
            let r = Record::new(self.mint_url.as_str(), ks.id.to_string(), None);
            r.to_counter(keysets).map(|c| counters.push(c)).unwrap();
        }

        // bip32 Key index, within [0, 2^31 - 1]
        counters.retain(|c| c.record.counter < (2u64.pow(31) - 1) - 50);

        // create timestamp asc
        counters.sort_by_key(|k| k.record.ts);

        // println!("{:?}", counters);

        self.counters = counters;
        self
    }

    // lock for write state only success use the counter
    pub fn start_count<'s, 'l: 's>(
        &'s mut self,
        unit: Option<&'l str>,
        keysets: &'l [KeySet],
    ) -> anyhow::Result<ManagerCounter<'s>> {
        let unit = unit.unwrap_or(CURRENCY_UNIT_SAT);
        let counter = self
            .counters
            .iter_mut()
            .find(|c| c.keyset(keysets).unit.as_str() == unit)
            .ok_or_else(|| format_err!("counters not find suitable keyset"))?;

        let mc = ManagerCounter {
            keyset: counter.keyset(keysets),
            mnemonic: self.mnemonic.as_ref(),
            counter,
        };

        Ok(mc)
    }
}

#[derive(Debug)]
pub struct ManagerCounter<'a> {
    keyset: &'a KeySet,
    counter: &'a mut Counter,
    mnemonic: Option<&'a Arc<MnemonicInfo>>,
}

impl<'a> Drop for ManagerCounter<'a> {
    fn drop(&mut self) {
        self.cancel()
    }
}

impl<'a> ManagerCounter<'a> {
    pub fn count(&mut self) -> u64 {
        self.counter.next()
    }

    pub fn now(&self) -> u64 {
        self.counter.state
    }

    pub fn before(&self) -> u64 {
        self.counter.record.counter
    }

    pub fn record(&self) -> &Record {
        &self.counter.record
    }

    pub fn keyset(&self) -> &KeySet {
        self.keyset
    }

    pub fn mnemonic(&self) -> Option<&'a Arc<MnemonicInfo>> {
        self.mnemonic
    }

    pub fn cancel(&mut self) {
        self.counter.state = self.counter.record.counter;
    }

    // // call store after?
    // pub fn comfirm(&mut self) {
    //     self.update = true;
    // }

    // pub async fn commit(&mut self, store: ()) -> anyhow::Result<usize> {
    pub async fn commit<'s, 'l: 's, S: RecordStore + 'l>(
        &'l mut self,
        store: S,
    ) -> anyhow::Result<usize>
    where
        S::Error: 'static,
    {
        self.counter.record.counter = self.counter.state;

        // commit to database
        if let Some(mi) = self.mnemonic {
            if self.counter.record.pubkey.is_empty() {
                self.counter.record.pubkey = mi.pubkey.to_owned();
            }
            store.add_record(&self.counter.record).await?;
        }
        Ok(0)
    }

    pub fn generate(&self, count: u64, amount: Amount) -> anyhow::Result<PreMint> {
        use cashu::dhke::blind_message;
        use cashu::nuts::nut01::SecretKey;
        use cashu::secret::Secret;

        let keyset = self.keyset;

        let secret;
        let blinding_factor;
        if let Some(mi) = &self.mnemonic {
            secret = Secret::from_seed(&mi.mnemonic, keyset.id, count)?;
            blinding_factor = SecretKey::from_seed(&mi.mnemonic, keyset.id, count)?;
        } else {
            secret = Secret::generate();
            blinding_factor = SecretKey::generate();
        }

        debug!(
            "{} {} {} {} {} {}",
            self.counter.record.mint,
            count,
            amount,
            keyset.id,
            secret.as_str(),
            self.mnemonic
                .as_ref()
                .map(|mi| mi.pubkey())
                .unwrap_or_default()
        );

        let (blinded, r) = blind_message(&secret.to_bytes(), Some(blinding_factor))?;

        let blinded_message = BlindedMessage::new(amount, keyset.id, blinded);

        let pre_mint = PreMint {
            blinded_message,
            secret: secret.clone(),
            r,
            amount,
        };

        Ok(pre_mint)
    }
}

pub type RecordStoreFake = ();

use std::error::Error as StdError;
#[async_trait]
pub trait RecordStore {
    type Error: Send + Sync + StdError + 'static;

    // counter records
    async fn add_record(&self, record: &Record) -> Result<(), Self::Error>;
    async fn delete_records(&self, mint_url: &Url) -> Result<(), Self::Error>;
    async fn get_records(&self, mint_url: &Url, pubkey: &str) -> Result<Vec<Record>, Self::Error>;
}

#[async_trait]
impl RecordStore for () {
    type Error = Infallible;
    async fn add_record(&self, _record: &Record) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn delete_records(&self, _mint_url: &Url) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn get_records(
        &self,
        _mint_url: &Url,
        _pubkey: &str,
    ) -> Result<Vec<Record>, Self::Error> {
        Ok(vec![])
    }
}
