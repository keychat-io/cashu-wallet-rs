pub use crate::wallet::MintUrl as Url;
pub use cashu;
use cashu::nuts::nut00;
pub use url::ParseError;

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::sync::Arc;
use std::sync::RwLock;

use crate::store::MintUrlWithUnit;
use crate::store::ProofsExtended;
use crate::wallet::ClientError;
use crate::wallet::MnemonicInfo;
use crate::wallet::SplitProofsGeneric;
use crate::wallet::WalletError;
use crate::wallet::CURRENCY_UNIT_SAT;
use crate::wallet::{AmountHelper, ProofsHelper, Token, Wallet};
use crate::wallet::{HttpOptions, MintClient};
use crate::wallet::{Proof, SplitProofsExtended};

use crate::store::impl_redb::StoreError;
use crate::store::MintUrlWithUnitOwned;
use crate::store::UnitedStore;

use crate::types::Mint;
use crate::types::{
    CashuTransaction, LNTransaction, Transaction, TransactionDirection, TransactionStatus,
};

use cashu::nuts::nut07::State;
use cashu::Bolt11Invoice;

#[derive(Debug)]
//
#[derive(strum::EnumIs, thiserror::Error)]
pub enum UniError<E: StdError = StoreError> {
    /// mint url unmatched
    #[error("Mint url unmatched")]
    MintUrlUnmatched,
    #[error("{0}")]
    Cashu(#[from] cashu::nuts::nut00::Error),
    /// mint client returns
    #[error("{0}")]
    Client(#[from] ClientError),
    #[error("Insufficant Funds")]
    InsufficientFunds,
    /// custum error
    #[error(transparent)]
    Custom(#[from] anyhow::Error),
    #[error("{0}")]
    Store(E),
}

impl<E: StdError> From<WalletError> for UniError<E> {
    fn from(value: WalletError) -> Self {
        match value {
            WalletError::Cashu(e) => UniError::Cashu(e),
            WalletError::Client(e) => UniError::Client(e),
            WalletError::InsufficientFunds => UniError::InsufficientFunds,
            WalletError::MintUrlUnmatched => UniError::MintUrlUnmatched,
            WalletError::Custom(e) => UniError::Custom(e),
        }
    }
}

impl<E: StdError> From<url::ParseError> for UniError<E> {
    fn from(err: url::ParseError) -> Self {
        ClientError::Url(err).into()
    }
}

pub(crate) type Error<E> = UniError<E>;

pub trait UniErrorFrom<S>:
    From<S::Error> + From<url::ParseError> + From<WalletError> + From<cashu::nuts::nut00::Error>
where
    S: UnitedStore,
{
}
impl<S> UniErrorFrom<S> for Error<S::Error>
where
    S: UnitedStore,
    Error<S::Error>: From<S::Error>
        + From<url::ParseError>
        + From<WalletError>
        + From<cashu::nuts::nut00::Error>,
{
}

/// multiple mints wallet
// #[derive(Debug)]
pub struct UnitedWallet<S>
where
    S: UnitedStore,
{
    store: S,
    http_options: Arc<HttpOptions>,
    mnemonic: Option<Arc<MnemonicInfo>>,
    wallets: RwLock<BTreeMap<String, Arc<Wallet>>>,
}

impl<S> UnitedWallet<S>
where
    S: UnitedStore,
{
    pub fn new(store: S, http_options: HttpOptions) -> Self {
        Self::with_mnemonic(store, http_options, None)
    }

    pub fn with_mnemonic(
        store: S,
        http_options: HttpOptions,
        mnemonic: Option<Arc<MnemonicInfo>>,
    ) -> Self {
        Self {
            store,
            mnemonic,
            http_options: Arc::new(http_options),
            wallets: Default::default(),
        }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn http_options(&self) -> &Arc<HttpOptions> {
        &self.http_options
    }

    pub fn mnemonic(&self) -> Option<&Arc<MnemonicInfo>> {
        self.mnemonic.as_ref()
    }

    /// current mint_urls in the UnitedWallet object
    pub fn mint_urls(&self) -> Result<Vec<String>, Error<S::Error>> {
        let urls = self
            .wallets
            .read()
            .map_err(|e| format_err!("wallets read {}", e))?
            .keys()
            .cloned()
            .collect();

        Ok(urls)
    }
}

impl<S> UnitedWallet<S>
where
    S: UnitedStore + Clone + Send + Sync + 'static,
    // S::Error: StdError + Into<Error<S::Error>>,
    Error<S::Error>: UniErrorFrom<S>,
{
    pub fn get_wallet(&self, url: &Url) -> Result<Arc<Wallet>, Error<S::Error>> {
        let wallet = self
            .get_wallet_optional(url)?
            .ok_or_else(|| format_err!("mint_url notfound"))?;

        Ok(wallet)
    }
    pub fn get_wallet_optional(&self, url: &Url) -> Result<Option<Arc<Wallet>>, Error<S::Error>> {
        let wallet = self
            .wallets
            .read()
            .map_err(|e| format_err!("wallets read {}", e))?
            .get(url.as_str())
            .cloned();

        Ok(wallet)
    }

    pub async fn update_mnmonic(
        &mut self,
        mnemonic: Option<Arc<MnemonicInfo>>,
    ) -> Result<bool, Error<S::Error>> {
        if self.mnemonic == mnemonic {
            return Ok(false);
        }

        let kvs = self
            .wallets
            .read()
            .map_err(|e| format_err!("wallets read {}", e))?
            .clone();

        let mut wallets = std::collections::BTreeMap::new();
        for (k, v) in &kvs {
            let mut w: Wallet = v.as_ref().clone();
            w.update_mnmonic(mnemonic.clone(), &self.store).await?;
            wallets.insert(k.clone(), w.into());
        }

        self.wallets = RwLock::new(wallets);
        let has = std::mem::replace(&mut self.mnemonic, mnemonic);
        Ok(has.is_some())
    }

    pub async fn add_mint(&self, mint_url: Url, reconnect: bool) -> Result<bool, Error<S::Error>> {
        self.add_mint_with_units(mint_url, reconnect, &[CURRENCY_UNIT_SAT], None)
            .await
    }
    pub async fn add_mint_with_units(
        &self,
        mint_url: Url,
        reconnect: bool,
        units: &[&str],
        mut wallet: Option<Arc<Wallet>>,
    ) -> Result<bool, Error<S::Error>> {
        let url = mint_url.as_str().to_owned();

        if wallet.is_none() {
            wallet = self.get_wallet_optional(&mint_url)?;
        }
        let has = wallet.is_some();
        if wallet.is_none() || reconnect {
            let client = MintClient::new(mint_url.clone(), self.http_options.as_ref().clone())?;

            let w = Wallet::new(client, None, None, self.mnemonic.clone(), self.store()).await?;

            if units.len() >= 1
                && w.keysets
                    .iter()
                    .all(|ks| !units.contains(&ks.unit.as_str()))
            {
                return Err(format_err!("currency units not supported").into());
            }

            let w = Arc::new(w);
            {
                let mut lock = self
                    .wallets
                    .write()
                    .map_err(|e| format_err!("wallets write {}", e))?;

                lock.insert(url.clone(), w.clone());
            }

            wallet = Some(w);
        }
        let w = wallet.unwrap();

        let mut mint = Mint::new(url.clone(), None);
        let record = self.store.get_mint(mint_url.as_str()).await?;
        let store = if let Some(r) = record {
            mint.active != r.active || r.info.is_none() || r.info.as_ref().unwrap() != &w.info
        } else {
            true
        };

        if store {
            mint.info = Some(w.info.clone());
            self.store.add_mint(&mint).await?;
        }

        Ok(has)
    }

    /// current mints in the UnitedWallet object
    pub async fn mints(&self) -> Result<Vec<Mint>, Error<S::Error>> {
        let mut mints = self.store.get_mints().await?;
        mints.retain(|m| m.active);
        Ok(mints)
    }

    /// load active mints from database::get_mints
    pub async fn load_mints_from_database(&self) -> Result<Vec<Mint>, Error<S::Error>> {
        let mints = self.mints().await?;

        for m in &mints {
            let mint_url = m.url.parse::<Url>()?;
            self.add_mint_with_units(mint_url, false, &[], None).await?;
        }

        Ok(mints)
    }

    /// remove the mint from object and set it as unactive in database
    pub async fn remove_mint(&self, mint_url: &Url) -> Result<bool, Error<S::Error>> {
        let wi = self.store().get_mint(mint_url.as_str()).await?;
        if let Some(mut wi) = wi {
            wi.active = false;
            self.store().add_mint(&wi).await?;
        }

        let mut lock = self
            .wallets
            .write()
            .map_err(|e| format_err!("wallets write {}", e))?;
        let w = lock.remove(mint_url.as_str());

        Ok(w.is_some())
    }

    // pub async fn update_mint(&self, mint: &Mint) -> Result<(), Error<S::Error>> {
    //     self.store.add_mint(mint).await?;
    //     Ok(())
    // }

    pub async fn get_balance_limit_unit(
        &self,
        mint_url: &Url,
        unit: Option<&str>,
    ) -> Result<u64, Error<S::Error>> {
        // let wallet = self.get_wallet(mint_url)?;

        let ps = self
            .store
            .get_proofs_limit_unit(mint_url, unit.unwrap_or(CURRENCY_UNIT_SAT))
            .await?;
        let balance = ps.sum().to_u64();

        Ok(balance)
    }
    pub async fn get_balance(
        &self,
        mint_url: &Url,
    ) -> Result<BTreeMap<String, u64>, Error<S::Error>> {
        let mps = self.store.get_proofs(mint_url).await?;

        let mut map: BTreeMap<_, _> = Default::default();
        for (k, ps) in mps {
            map.insert(k, ps.sum().to_u64());
        }
        Ok(map)
    }

    pub async fn get_balances(
        &self,
    ) -> Result<BTreeMap<MintUrlWithUnitOwned, u64>, Error<S::Error>> {
        let mps = self.store.get_all_proofs().await?;

        let mut map: BTreeMap<_, _> = Default::default();
        for (k, ps) in mps {
            map.insert(k, ps.sum().to_u64());
        }

        let mut mints = self.store.get_mints().await?;
        mints.retain(|m| m.active);

        for m in &mints {
            let mint_url = m.url.parse::<Url>()?;
            if let Some(i) = &m.info {
                let nut04 = &i.nuts.nut04;
                for m in &nut04.methods {
                    let unit = m.unit.as_str();
                    let mu = MintUrlWithUnit::new(mint_url.as_str(), unit);
                    if !map.contains_key(&mu) {
                        map.insert(mu.into_owned(), 0);
                    }
                }
            }
        }

        Ok(map)
    }

    pub async fn receive_tokens(&self, cashu_tokens: &str) -> Result<u64, Error<S::Error>> {
        let mut txs = vec![];
        self.receive_tokens_full(cashu_tokens, &mut txs).await?;
        Ok(txs.iter().map(|tx| tx.amount()).sum())
    }

    pub async fn receive_tokens_full(
        &self,
        cashu_tokens: &str,
        txs: &mut Vec<Transaction>,
    ) -> Result<(), Error<S::Error>> {
        self.receive_tokens_full_limit_unit(cashu_tokens, txs, &[])
            .await
    }

    #[doc(hidden)]
    pub async fn receive_tokens_full_limit_unit(
        &self,
        cashu_tokens: &str,
        txs: &mut Vec<Transaction>,
        units: &[&str],
    ) -> Result<(), Error<S::Error>> {
        let tokens: Token = cashu_tokens.parse()?;

        let unit = tokens.unit.as_ref().map(|s| s.as_str());
        if units.len() >= 1 && !units.contains(&unit.unwrap_or(CURRENCY_UNIT_SAT)) {
            return Err(Error::Custom(format_err!(
                "not support the currency uint: {}",
                unit.unwrap_or(CURRENCY_UNIT_SAT)
            )));
        }

        for token in &tokens.token {
            let a = token.proofs.sum().to_u64();
            let mint_url = &token.mint;

            let wallet = self.get_wallet(mint_url)?;

            let ps = wallet.receive_token(token, unit, &self.store).await?;
            let ps = ps.into_extended_with_unit(unit);
            self.store.add_proofs(&mint_url, &ps).await?;

            let token_str = Wallet::proofs_to_token(
                &token.proofs,
                mint_url.clone(),
                tokens.memo.clone(),
                unit,
            )?;

            let tx = CashuTransaction::new(
                TransactionStatus::Success,
                TransactionDirection::In,
                a,
                mint_url.as_str(),
                &token_str,
                None,
                unit,
            )
            .into();
            self.store.add_transaction(&tx).await?;

            txs.push(tx);
        }

        Ok(())
    }

    pub async fn send_tokens(
        &self,
        mint_url: &Url,
        amount: u64,
        memo: Option<String>,
        unit: Option<&str>,
        info: Option<String>,
    ) -> Result<Transaction, Error<S::Error>> {
        self.send_tokens_full(mint_url, amount, memo, unit, info, true)
            .await
    }
    pub async fn send_tokens_full(
        &self,
        mint_url: &Url,
        amount: u64,
        memo: Option<String>,
        unit: Option<&str>,
        info: Option<String>,
        allow_skip_split: bool,
    ) -> Result<Transaction, Error<S::Error>> {
        let mut wallet = self.get_wallet_optional(mint_url)?;
        let unit = unit.unwrap_or(CURRENCY_UNIT_SAT);

        let mut ps = self.store.get_proofs_limit_unit(mint_url, unit).await?;
        let select = select_send_proofs(amount, &mut ps)?;
        let pss = &ps[..=select];

        let tokens = if pss.sum().to_u64() == amount && allow_skip_split {
            SplitProofsExtended::new(pss.to_owned(), 0)
        } else {
            if wallet.is_none() {
                wallet = Some(self.get_wallet(mint_url)?);
            }
            wallet
                .as_ref()
                .unwrap()
                .send(amount.into(), pss, Some(unit), &self.store)
                .await?
        };

        self.store.add_proofs(mint_url, tokens.keep()).await?;
        self.store.delete_proofs(mint_url, pss).await?;

        let cashu_tokens =
            Wallet::proofs_to_token(tokens.send(), mint_url.clone(), memo, Some(unit))?;

        let mut tx: Transaction = CashuTransaction::new(
            TransactionStatus::Pending,
            TransactionDirection::Out,
            amount,
            mint_url.as_str(),
            &cashu_tokens,
            None,
            Some(unit),
        )
        .into();
        *tx.info_mut() = info;

        self.store.add_transaction(&tx).await?;

        Ok(tx)
    }

    pub async fn prepare_one_proofs(
        &self,
        mint_url: &Url,
        amount: u64,
        unit: Option<&str>,
    ) -> Result<u64, Error<S::Error>> {
        self.prepare_denomination_proofs(mint_url, amount, unit, 1)
            .await
    }

    // maybe to do, denomination is 2 can't handle many 1
    async fn prepare_denomination_proofs(
        &self,
        mint_url: &Url,
        amount: u64,
        currency_unit: Option<&str>,
        denomination: u64,
    ) -> Result<u64, Error<S::Error>> {
        if denomination <= 0 {
            return Err(format_err!("prepare_denomination_proofs denomination shoud bg 0").into());
        }

        let mut count_before = 0u64;

        let wallet = self.get_wallet(mint_url)?;

        let mut ps = self
            .store
            .get_proofs_limit_unit(mint_url, currency_unit.unwrap_or(CURRENCY_UNIT_SAT))
            .await?;
        ps.retain(|p| {
            let is = p.as_ref().amount.to_u64() == denomination;
            if is {
                count_before += 1;
            }
            !is
        });

        let mut count_splits = 0u64;
        if count_before * denomination < amount {
            let amount = amount - count_before * denomination;

            let select = select_send_proofs(amount, &mut ps)?;
            let pss = &ps[..=select];

            let tokens = wallet
                .send_with_denomination(
                    amount.into(),
                    pss,
                    denomination.into(),
                    currency_unit,
                    &self.store,
                )
                .await?;

            self.store.add_proofs(mint_url, tokens.all()).await?;
            self.store.delete_proofs(mint_url, pss).await?;

            for i in tokens.all() {
                if i.as_ref().amount.to_u64() == denomination {
                    count_splits += 1;
                }
            }
        }

        Ok(count_before + count_splits)
    }

    pub async fn check_pendings(&self) -> Result<(usize, usize), Error<S::Error>> {
        let pendings = self.store.get_pending_transactions().await?;
        // pendings.sort_unstable_by(|a, b|a.mint_url().cmp(&b.mint_url()));

        let pendings_count = pendings.len();
        let mut update_count = 0;

        let mut cushs: BTreeMap<String, Vec<Transaction>> = BTreeMap::new();
        let mut lns: BTreeMap<String, Vec<Transaction>> = BTreeMap::new();
        for tx in pendings {
            if tx.is_cashu() {
                let txs = cushs.entry(tx.mint_url().to_owned()).or_default();

                txs.push(tx)
            } else if tx.is_ln() {
                let txs = lns.entry(tx.mint_url().to_owned()).or_default();

                txs.push(tx)
            } else {
                unreachable!()
            }
        }

        let batch_size = 64;
        for (k, txs) in cushs.iter_mut() {
            let mint_url = k.parse()?;
            let wallet = self.get_wallet_optional(&mint_url)?;
            if wallet.is_none() {
                continue;
            }
            let wallet = wallet.unwrap();

            let mut ps = Vec::with_capacity(batch_size);
            let mut tokens = Vec::with_capacity(txs.len());
            let txc = txs.len();

            for i in 0..txs.len() {
                let token: Token = txs[i].content().parse()?;

                let mut psc = 0;
                // prevent empty
                for t in token.token {
                    if t.mint != mint_url {
                        Err(WalletError::MintUrlUnmatched)?;
                    }

                    psc += t.proofs.len();
                    for p in t.proofs {
                        ps.push(p);
                    }
                }
                tokens.push(psc);

                let tokens_sum = tokens.iter().sum::<usize>();
                assert_eq!(tokens_sum, ps.len());

                // println!("txc: {}, {}", txc, tokens_sum);

                if i + 1 >= txc || tokens_sum >= batch_size {
                    let state = wallet.check_proofs(&ps).await?;
                    if state.states.len() != ps.len() {
                        return Err(format_err!(
                            "invalid check_proofs response {}->{}",
                            ps.len(),
                            state.states.len(),
                        )
                        .into());
                    }

                    let txs = &mut txs[i + 1 - tokens.len()..=i];
                    let mut offset = 0;
                    for (idx, token) in tokens.iter().enumerate() {
                        let tx = &mut txs[idx];
                        let pss = &state.states[offset..offset + *token];
                        // #[rustfmt::skip]
                        // info!("{} {} [{}..{}]: {:?} {} {} {}", idx, token, offset, offset+token, pss, tx.id(), tx.direction(), tx.status());

                        let is_spent = pss.iter().any(|b| b.state == State::Spent);
                        if is_spent {
                            *tx.status_mut() = TransactionStatus::Success;
                            // println!("{:?}", tx);

                            self.store.add_transaction(&*tx).await?;
                            update_count += 1;
                        }

                        offset += *token;
                    }

                    ps.clear();
                    tokens.clear();
                }
            }
        }

        for (k, txs) in lns.iter_mut() {
            let mint_url = k.parse()?;
            // let _wallet = self.get_wallet(&mint_url)?;

            for tx in txs {
                let res = self
                    .mint_tokens(&mint_url, tx.amount(), tx.id().to_owned(), tx.unit())
                    .await;
                if res.is_ok() {
                    update_count += 1;
                }
            }
        }

        Ok((update_count, pendings_count))
    }

    pub async fn check_proofs_in_database(&self) -> Result<(usize, usize), Error<S::Error>> {
        let ps = self.store.get_all_proofs().await?;

        let all_count = ps.values().map(|m| m.len()).sum();
        let mut update_count = 0;

        let batch_size = 64;
        for (k, txs) in ps.iter() {
            let mint_url = k.mint().parse()?;
            let wallet = self.get_wallet_optional(&mint_url)?;
            if wallet.is_none() {
                continue;
            }
            let wallet = wallet.unwrap();

            for ps in txs.chunks(batch_size) {
                let state = wallet.check_proofs(ps).await?;
                if state.states.len() != ps.len() {
                    return Err(format_err!(
                        "invalid check_proofs response {}->{}",
                        ps.len(),
                        state.states.len(),
                    )
                    .into());
                }

                for (idx, b) in state.states.into_iter().enumerate() {
                    let is_spent = b.state == State::Spent;
                    if is_spent {
                        let tx = &ps[idx..=idx];

                        self.store.delete_proofs(&mint_url, tx).await?;
                        update_count += 1;
                    }
                }
            }
        }

        Ok((update_count, all_count))
    }

    pub async fn request_mint(
        &self,
        mint_url: &Url,
        amount: u64,
        unit: Option<&str>,
    ) -> Result<Transaction, Error<S::Error>> {
        let wallet = self.get_wallet(mint_url)?;
        let unit = unit.unwrap_or(CURRENCY_UNIT_SAT);

        let pr = wallet.request_mint(amount.into(), Some(unit), None).await?;

        let tx = LNTransaction::new(
            TransactionStatus::Pending,
            TransactionDirection::In,
            amount,
            None,
            mint_url.as_str(),
            &pr.request,
            &pr.quote,
            None,
            Some(unit),
        )
        .into();

        self.store.add_transaction(&tx).await?;

        Ok(tx)
    }

    pub async fn mint_tokens(
        &self,
        mint_url: &Url,
        amount: u64,
        hash: String,
        _unit: Option<&str>,
    ) -> Result<Transaction, Error<S::Error>> {
        let wallet = self.get_wallet(mint_url)?;

        let mut tx = self.store.get_transaction(&hash).await?;

        let tokens = match wallet
            .mint_token(
                amount.into(),
                tx.as_ref().and_then(|tx| tx.unit()).or(_unit),
                &hash,
                None,
                &self.store,
            )
            .await
        {
            Ok(t) => t,
            Err(e) => {
                // https://github.com/cashubtc/nutshell/blob/0.10.0/cashu/core/errors.py#L21 "invoice not paid."
                // https://github.com/cashubtc/nutshell/blob/0.12.3/cashu/core/errors.py#L19 "invoice not paid."
                // https://github.com/cashubtc/nutshell/blob/0.13.0/cashu/core/errors.py#L78 "Lightning invoice not paid yet."
                // https://github.com/cashubtc/cashu-feni/blob/master/mint/mint.go#L251C27-L251C60 "Lightning invoice not paid yet."
                let unpaid = match &e {
                    WalletError::Client(ClientError::Mint(_ec, ed)) => ed.contains("not paid"),
                    _ => false,
                };

                if unpaid {
                    if let Some(tx) = &mut tx {
                        let invoice: Bolt11Invoice = tx
                            .content()
                            .parse()
                            .map_err(|e| format_err!("invalid invoice: {}", e))?;

                        if invoice.is_expired() {
                            *tx.status_mut() = TransactionStatus::Expired;
                            self.store().add_transaction(tx).await?;
                        }
                    }
                }

                return Err(e.into());
            }
        };

        for t in &tokens.token {
            self.store.add_proofs(mint_url, &t.proofs).await?;
        }

        if let Some(ref mut tx) = tx {
            if !tx.is_ln() {
                return Err(format_err!("the transaction is cashu").into());
            }

            *tx.status_mut() = TransactionStatus::Success;
            self.store.add_transaction(tx).await?;
        } else {
            // save with empty pr
            let txfake = LNTransaction::new(
                TransactionStatus::Success,
                TransactionDirection::In,
                amount,
                None,
                mint_url.as_str(),
                "",
                &hash,
                None,
                tokens.unit.as_ref().map(|s| s.as_str()),
            )
            .into();

            self.store.add_transaction(&txfake).await?;
            tx = Some(txfake);
        }

        Ok(tx.unwrap())
    }

    // repeat melt will get 20000 Lightning payment unsuccessful.
    pub async fn melt(
        &self,
        mint_url: &Url,
        invoice_str: String,
        amount: Option<u64>,
        unit: Option<&str>,
        quote_response: Option<&mut cashu::nuts::MeltQuoteBolt11Response>,
    ) -> Result<Transaction, Error<S::Error>> {
        let invoice: Bolt11Invoice = invoice_str
            .parse()
            .map_err(|e| format_err!("Invoice decode: {}", e))?;
        if invoice.is_expired() {
            return Err(format_err!("Invoice expired").into());
        }

        let unit = unit.unwrap_or(CURRENCY_UNIT_SAT);
        // https://github.com/lightning/bolts/blob/master/11-payment-encoding.md#rationale
        let amount = if let Some(amount_msats) = invoice.amount_milli_satoshis() {
            // ceil
            let amount_in_invoice = amount_msats / 1000 + (amount_msats % 1000 > 0) as u64;
            if let Some(a) = amount {
                if a != amount_in_invoice {
                    return Err(format_err!("amount unmatch {}/{}", a, amount_in_invoice).into());
                }
            }
            amount_in_invoice
        } else {
            // https://8333.space:3338 no support
            // melt 400: {"detail":"invoice has no amount.","code":0}
            // if amount.is_none() {
            return Err(format_err!("invoice has no amount.").into());
            // }
            // amount.unwrap()
        };

        let wallet = self.get_wallet(mint_url)?;
        let form = wallet.request_melt(&invoice, Some(unit), None).await?;
        let mut fee = form.fee_reserve;
        if let Some(q) = quote_response {
            *q = form.clone();
        }

        let amount_with_fee = amount + fee;

        let mut ps = self.store.get_proofs_limit_unit(mint_url, unit).await?;
        let select = select_send_proofs(amount_with_fee, &mut ps)?;
        let ps = &ps[..=select];

        let amount_selected = ps.sum();

        // #[rustfmt::skip]
        // println!("{}+{}=>{}/{}", amount, fee, amount_with_fee, amount_selected.to_u64());

        // or depents on nut08?
        // let fee_and_remains = ps.sum() - cashu::Amount::from_sat(amount);
        // or spit fisrt
        let ps2 = if amount_selected.to_u64() > amount_with_fee {
            let psnew = wallet
                .send(amount_with_fee.into(), ps, Some(unit), &self.store)
                .await?;
            self.store.add_proofs(mint_url, &psnew.proofs).await?;
            self.store.delete_proofs(mint_url, ps).await?;
            psnew
        } else {
            SplitProofsGeneric::new(ps.to_owned(), 0)
        };

        let pm = wallet
            .melt(
                &form.quote,
                ps2.send(),
                fee.into(),
                Some(unit),
                None,
                &self.store,
            )
            .await?;
        if let Some(remain) = pm.change {
            let remain = remain.into_extended_with_unit(Some(unit));
            self.store.add_proofs(mint_url, &remain).await?;
            let ra = remain.sum();
            if fee >= ra.to_u64() {
                fee -= ra.to_u64();
            }
        }

        if pm.paid {
            self.store.delete_proofs(mint_url, ps2.send()).await?;
            // return Err(format_err!("mint server reponse not paid").into());
        }

        // fill a hash
        let hash = form.quote;

        let txln: Transaction = LNTransaction::new(
            if pm.paid {
                TransactionStatus::Success
            } else {
                TransactionStatus::Failed
            },
            TransactionDirection::Out,
            amount,
            Some(fee),
            mint_url.as_str(),
            &invoice_str,
            &hash,
            None,
            Some(unit),
        )
        .into();
        self.store.add_transaction(&txln).await?;

        Ok(txln)
    }

    /// sleepms_after_check_a_batch for (code: 429): {"detail":"Rate limit exceeded."}
    /// 1. brefore call api f: (url, keysets.len(), idx, keysetid, unit, before, batch, now, pre_mints, None..) -> exit
    /// 2. after call api f: (url, keysets.len(), idx, keysetid, unit, before, batch, now, pre_mints, api-outputs, api-signatures, None) -> exit
    /// 3. after construct proofs(&&after call checkState): (url, keysets.len(), idx, keysetid, unit, before, batch, now, None.., proofs) -> exit
    pub async fn restore(
        &self,
        mint_url: &Url,
        batch_size: u64,
        sleepms_after_check_a_batch: u64,
        keysetids: &[String],
        mi: Option<Arc<MnemonicInfo>>,
        f: impl Fn(
            &str,
            usize,
            usize,
            &str,
            &str,
            u64,
            u64,
            u64,
            Option<&Vec<nut00::PreMint>>,
            Option<&Vec<nut00::BlindedMessage>>,
            Option<&Vec<nut00::BlindSignature>>,
            Option<&ProofsExtended>,
        ) -> bool,
    ) -> Result<ProofsExtended, Error<S::Error>> {
        let w = self.get_wallet(mint_url)?;
        let mut proofs = Vec::new();

        let res = w
            .restore(
                &mut proofs,
                &self.store,
                batch_size,
                sleepms_after_check_a_batch,
                keysetids,
                mi,
                f,
            )
            .await;

        if !proofs.is_empty() {
            let ps = self.store().get_proofs(mint_url).await?;
            let psmap = ps
                .values()
                .map(|v| v.iter().map(|p| (&p.raw.secret, p)))
                .flatten()
                .collect::<std::collections::BTreeMap<_, _>>();

            // prevent duplicate store
            proofs.retain(|p| psmap.get(&p.raw.secret).is_none());
            self.store.add_proofs(mint_url, &proofs).await?;
        }

        let () = res?;

        Ok(proofs)
    }
}

// simple
#[doc(hidden)]
pub fn select_send_proofs<E: StdError>(
    amount: u64,
    proofs: &mut Vec<impl AsRef<Proof>>,
) -> Result<usize, Error<E>> {
    if amount == 0 {
        return Err(WalletError::Custom(format_err!("send amount 0")).into());
    }

    let mut a = 0;
    let mut take = 0;

    let p = proofs
        .iter()
        .position(|p| p.as_ref().amount.to_u64() == amount);
    if let Some(p) = p {
        proofs.swap(0, p);
    } else {
        for (idx, proof) in proofs.iter().enumerate() {
            a += proof.as_ref().amount.to_u64();

            if a >= amount {
                take = idx;
                break;
            }
        }

        if a < amount {
            return Err(WalletError::insufficant_funds().into());
        }
    }

    Ok(take)
}
