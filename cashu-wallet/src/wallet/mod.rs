use crate::types::MintInfo;
use cashu::dhke::unblind_message;
use cashu::nuts::nut01::Keys;
use cashu::nuts::nut02::KeySet;
use cashu::nuts::nut02::KeySetVersion;
use cashu::nuts::*;
use cashu::types::Melted;
use cashu::Amount;
use cashu::Bolt11Invoice;

use error::WalletError as Error;
use std::sync::Arc;
use tokio::sync::Mutex;

mod client;
mod counter;
mod error;
mod token;

pub use cashu::nuts::{PreMintSecrets, Proof, Proofs};
pub use token::{
    MintProofs,
    MintProofsGeneric,
    //
    MintUrl,
    //
    ProofExtended,
    ProofsExtended,
    ProofsHelper,
    //
    Token,
    TokenExtened,
    TokenGeneric,
};

pub use client::*;
pub use counter::*;
pub use error::*;

/// helper for Amount
pub trait AmountHelper {
    fn to_u64(&self) -> u64;
}

impl AmountHelper for Amount {
    fn to_u64(&self) -> u64 {
        (*self).into()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BlindedMessages<'a> {
    // #[serde(flatten)]
    pub secrets: &'a [PreMint],
}

impl<'a> BlindedMessages<'a> {
    pub fn new(secrets: &'a [PreMint]) -> Self {
        Self { secrets }
    }
}

use std::fmt;
impl fmt::Display for BlindedMessages<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_list();
        for e in self.secrets {
            f.entry(&e.blinded_message);
        }
        f.finish()
    }
}

use serde::ser::{Serialize, SerializeSeq, Serializer};
impl<'a> Serialize for BlindedMessages<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.secrets.len()))?;
        for e in self.secrets {
            seq.serialize_element(&e.blinded_message)?;
        }
        seq.end()
    }
}

#[derive(Debug, Clone)]
pub struct Wallet {
    pub(super) client: MintClient,
    pub(super) keysets: Vec<KeySet>,
    pub(super) info: MintInfo,
    pub(super) counter: Option<Arc<Mutex<Manager>>>,
}

impl Wallet {
    pub async fn new(
        client: MintClient,
        mut keysets: Option<Vec<KeySet>>,
        mut info: Option<MintInfo>,
        mnemonic: Option<Arc<MnemonicInfo>>,
        store: impl RecordStore,
    ) -> Result<Self, Error> {
        if keysets.is_none() {
            let ks = client.get_keys(None).await?;
            keysets = Some(ks.keysets);
        }

        if info.is_none() {
            let mi = client.get_info().await?;
            info = Some(mi);
        }

        // old base64 keysetid can't convert as u64, or remove?
        keysets
            .as_mut()
            .map(|ks| ks.retain(|k| k.id.version != nut02::KeySetVersion::VersionBs));
        // .map(|ks| ks.sort_by_key(|k| k.id.version != nut02::KeySetVersion::VersionBs));

        let keysets = keysets.unwrap();
        if keysets.len() < 1 {
            return Err(format_err!("empty keysets").into());
        }

        let mut this = Self {
            client,
            keysets,
            info: info.unwrap(),
            counter: None,
        };

        this.update_mnmonic(mnemonic, store).await?;

        Ok(this)
    }

    pub async fn update_mnmonic(
        &mut self,
        mnemonic: Option<Arc<MnemonicInfo>>,
        store: impl RecordStore,
    ) -> Result<bool, Error> {
        let mut records = vec![];
        if let Some(mi) = &mnemonic {
            records = store
                .get_records(self.client.url(), mi.pubkey())
                .await
                .map_err(|e| Error::Custom(e.into()))?;
        }

        let counter = Manager::new(self.client.url())
            .mnemonic(mnemonic)
            .records(records, &self.keysets);

        let has = std::mem::replace(&mut self.counter, Some(Arc::new(Mutex::new(counter))));

        Ok(has.is_some())
    }

    pub fn client(&self) -> &MintClient {
        &self.client
    }

    pub async fn keyset0(&self, unit: Option<&str>) -> Result<&KeySet, Error> {
        let unit = unit.unwrap_or(CURRENCY_UNIT_SAT);

        let lock = self.counter.as_ref().unwrap().lock().await;
        let ks = lock
            .counters
            .iter()
            .map(|c| c.keyset(&self.keysets))
            .find(|k| k.unit.as_str() == unit)
            .ok_or_else(|| Error::Custom(format_err!("counters not find suitable keyset")))?;

        Ok(ks)
    }

    /// Request Token Mint
    pub async fn request_mint(
        &self,
        amount: Amount,
        unit: Option<&str>,
        method: Option<&str>,
    ) -> Result<nut04::MintQuoteBolt11Response, Error> {
        if self.info.nuts.nut04.disabled {
            return Err(format_err!("token mint disabled").into());
        }
        Ok(self
            .client
            .request_mint(
                amount,
                unit.unwrap_or(CURRENCY_UNIT_SAT),
                method.unwrap_or(PAYMEN_METHOD_BOLT11),
            )
            .await?)
    }

    /// Mint Proofs
    pub async fn mint<'s, 'l: 's>(
        &'l self,
        amount: Amount,
        counter: &'s mut ManagerCounter<'l>,
        hash: &'l str,
        method: Option<&'l str>,
    ) -> Result<ProofsExtended, Error> {
        let outputs = PreMintSecretsHyper::split_amount(amount, counter)?;
        let blinds = BlindedMessages::new(&outputs.secrets);

        let mint_res = self
            .client
            .mint(&blinds, hash, method.unwrap_or(PAYMEN_METHOD_BOLT11))
            .await?;

        let ps = process_swap_response::<ProofExtended>(
            outputs,
            mint_res.signatures,
            &counter.keyset().keys,
        )?;
        Ok(ps.into_extended_with_unit(Some(counter.keyset().unit.as_str())))
    }

    /// Mint Token
    pub async fn mint_token(
        &self,
        amount: Amount,
        unit: Option<&str>,
        hash: &str,
        method: Option<&str>,
        store: impl RecordStore,
    ) -> Result<TokenExtened, Error> {
        let mut lock = self.counter.as_ref().unwrap().lock().await;
        let mut counter = lock.start_count(unit, &self.keysets)?;

        let proofs = self.mint(amount, &mut counter, hash, method).await?;
        counter.commit(store).await?;

        let token = TokenGeneric::new(
            self.client.url.clone(),
            proofs,
            None,
            Some(counter.keyset().unit.clone()),
        )?;
        Ok(token)
    }

    /// Check if proofs state
    pub async fn check_state(&self, ys: &[PublicKey]) -> Result<nut07::CheckStateResponse, Error> {
        let status = self.client.check_state(ys).await?;

        Ok(status)
    }

    /// Check if proofs state
    pub async fn check_proofs(
        &self,
        proofs: impl ProofsHelper,
    ) -> Result<nut07::CheckStateResponse, Error> {
        let proofs = proofs.as_slice();
        let mut ys = Vec::with_capacity(proofs.len());
        for p in proofs {
            let p = p.as_ref();
            // println!("{:?}", p);
            let ss = p.secret.as_str();

            // let secret = if ss.len() == 64 {
            //     ss.parse::<SecretKey>()
            // } else {
            //     use base64::{alphabet, engine::general_purpose};

            //     let decode_config = general_purpose::GeneralPurposeConfig::new()
            //         .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent);
            //     let decoded =
            //         general_purpose::GeneralPurpose::new(&alphabet::STANDARD, decode_config)
            //             .decode(ss)
            //             .map_err(|e| Error::Custom(e.into()))?;
            //     SecretKey::from_slice(&decoded)
            // }
            // .map_err(|e| Error::Custom(e.into()))?;
            // let s = secret.to_string();

            // base64 secret string is compat also..
            let y = cashu::dhke::hash_to_curve(ss.as_bytes())?;
            // let y = cashu::dhke::hash_to_curve(secret.as_secret_bytes())?;
            // let y = hash_to_curve_pre1(secret.as_secret_bytes())?;

            // println!("p: {}", secret.public_key());
            // println!("y: {}", y);
            ys.push(y);
        }

        let status = self.client.check_state(&ys).await?;

        Ok(status)
    }

    /// Receive tokens belongs this url
    pub async fn receive(
        &self,
        token: &Token,
        proofs: &mut ProofsExtended,
        store: impl RecordStore + Clone,
    ) -> Result<(), Error> {
        let unit = &token.unit;
        for token in &token.token {
            let ps = self
                .receive_token(token, unit.as_ref().map(|s| s.as_str()), store.clone())
                .await?;
            proofs.extend(ps);
        }
        Ok(())
    }

    /// Receive token belongs this url
    pub async fn receive_token(
        &self,
        token: &MintProofs,
        unit: Option<&str>,
        store: impl RecordStore,
    ) -> Result<ProofsExtended, Error> {
        let mut ps = vec![];

        if token.proofs.is_empty() {
            return Ok(ps);
        }

        if token.mint.as_str() != self.client.url.as_str() {
            Err(Error::MintUrlUnmatched)?;
        }

        let amount = token.proofs.sum();
        if amount.to_u64() <= 0 {
            return Ok(ps);
        }

        let mut lock = self.counter.as_ref().unwrap().lock().await;
        let mut counter = lock.start_count(unit, &self.keysets)?;

        // let outputs = PreMintSecretsHyper::split_amount(amount, &mut counter)?;
        // let blinds = BlindedMessages::new(&outputs.secrets);

        // let swap_response = self.client.swap(&token.proofs, &blinds).await?;
        // counter.commit(store).await?;

        let (outputs, swap_response) = try_to_call_swap(
            self.client(),
            &token.proofs,
            amount,
            0.into(),
            0.into(),
            &mut counter,
            store,
        )
        .await?;

        if swap_response.signatures.is_empty() {
            let e = format_err!("Empty swap response");
            return Err(e.into());
        }

        ps = process_swap_response(
            outputs.messages,
            swap_response.signatures,
            &counter.keyset().keys,
        )?;

        Ok(ps.into_extended_with_unit(Some(counter.keyset().unit.as_str())))
    }

    /// Send: proofs select should do by caller, if not need swap should't call this
    pub async fn send(
        &self,
        amount: Amount,
        proofs: impl ProofsHelper + Copy,
        currency_unit: Option<&str>,
        store: impl RecordStore,
    ) -> Result<SplitProofsExtended, Error> {
        self.send_with_denomination(amount, proofs, 0.into(), currency_unit, store)
            .await
    }
    /// Send: proofs select should do by caller, if not need swap should't call this
    pub async fn send_with_denomination(
        &self,
        amount: Amount,
        proofs: impl ProofsHelper + Copy,
        denomination: Amount,
        currency_unit: Option<&str>,
        store: impl RecordStore,
    ) -> Result<SplitProofsExtended, Error> {
        let amount_available = proofs.sum();

        if amount_available < amount {
            return Err(Error::insufficant_funds());
        }

        // no need to split, buts could use to merge many small proofs to large 2^N proofs
        // if amount_available.eq(&amount)
        //     && (denomination == Amount::ZERO
        //         || proofs
        //             .as_slice()
        //             .iter()
        //             .all(|p| p.as_ref().amount <= denomination))
        // {
        //     let send = SplitProofsGeneric::new(proofs.into_extended_with_unit(currency_unit), 0);
        //     return Ok(send);
        // }

        let mut lock = self.counter.as_ref().unwrap().lock().await;
        let mut counter = lock.start_count(currency_unit, &self.keysets)?;

        let amount_to_keep = amount_available - amount;

        // let outputs =
        //     PreMintSecretsHyper::split_amount2(amount_to_keep, amount, denomination, &mut counter)?;
        // let blinds = BlindedMessages::new(&outputs.messages.secrets);
        // let swap_response = self.client.swap(proofs, &blinds).await?;
        // counter.commit(store).await?;
        let (outputs, swap_response) = try_to_call_swap(
            self.client(),
            proofs,
            amount_to_keep,
            amount,
            denomination,
            &mut counter,
            store,
        )
        .await?;

        if swap_response.signatures.is_empty() {
            let e = format_err!("Empty split response");
            return Err(e.into());
        }
        let proofs = process_swap_response::<ProofExtended>(
            outputs.messages,
            swap_response.signatures,
            &counter.keyset().keys,
        )?;

        let split = SplitProofsExtended::new(
            proofs.into_extended_with_unit(Some(counter.keyset().unit.as_str())),
            outputs.send_idx_start,
        );

        Ok(split)
    }

    /// 05 	Melting tokens: checkfees
    /// 05 	Melting tokens: Melt quote
    /// https://github.com/cashubtc/nuts/blob/main/05.md
    pub async fn request_melt(
        &self,
        invoice: &Bolt11Invoice,
        unit: Option<&str>,
        method: Option<&str>,
    ) -> Result<nut05::MeltQuoteBolt11Response, Error> {
        let resp = self
            .client
            .request_melt(
                invoice,
                unit.unwrap_or(CURRENCY_UNIT_SAT),
                method.unwrap_or(PAYMEN_METHOD_BOLT11),
            )
            .await?;
        Ok(resp)
    }

    pub async fn melt(
        &self,
        quote: &str,
        proofs: impl ProofsHelper,
        fee_reserve: Amount,
        unit: Option<&str>,
        method: Option<&str>,
        store: impl RecordStore,
    ) -> Result<Melted, Error> {
        let keyset0 = self.keyset0(unit).await?;

        let mut lock = self.counter.as_ref().unwrap().lock().await;
        let mut counter = lock.start_count(unit, &self.keysets)?;
        let mut outputs = PreMintSecretsHyper::split_blank(fee_reserve, &mut counter)?;
        // let blinds = BlindedMessages::new(&outputs.secrets);

        for p in &mut outputs.secrets {
            p.amount = 1.into();
        }
        let blinds = BlindedMessages::new(&outputs.secrets);

        let melt_response = self
            .client
            .melt(
                proofs,
                quote,
                Some(&blinds),
                method.unwrap_or(PAYMEN_METHOD_BOLT11),
            )
            .await?;

        let change_proofs = match melt_response.change {
            Some(change) => {
                let ps = process_swap_response(outputs, change, &keyset0.keys)?;
                Some(ps)
            }
            None => None,
        };

        // skip some is work
        counter.commit(store).await?;

        let melted = Melted {
            paid: melt_response.paid,
            preimage: melt_response.payment_preimage,
            change: change_proofs,
        };

        Ok(melted)
    }

    pub fn proofs_to_token(
        proofs: impl ProofsHelper,
        url: MintUrl,
        memo: Option<String>,
        unit: Option<&str>,
    ) -> Result<String, Error> {
        let unit = unit.unwrap_or(CURRENCY_UNIT_SAT).into();
        let t = TokenGeneric::new(url, proofs, memo, Some(unit))?;
        Ok(t.to_string())
    }

    /// sleepms_after_check_a_batch for (code: 429): {"detail":"Rate limit exceeded."}
    /// 1. brefore call api f: (url, keysets.len(), idx, keysetid, unit, before, batch, now, pre_mints, None..) -> exit
    /// 2. after call api f: (url, keysets.len(), idx, keysetid, unit, before, batch, now, pre_mints, api-outputs, api-signatures, None) -> exit
    /// 3. after construct proofs(&&after call checkState): (url, keysets.len(), idx, keysetid, unit, before, batch, now, None.., proofs) -> exit
    pub async fn restore(
        &self,
        proofs: &mut ProofsExtended,
        store: impl RecordStore + Copy,
        batch_size: u64,
        sleepms_after_check_a_batch: u64,
        keysetids: &[String],
        mut mi: Option<Arc<MnemonicInfo>>,
        f: impl Fn(
            &str,
            usize,
            usize,
            &str,
            &str,
            u64,
            u64,
            u64,
            Option<&Vec<PreMint>>,
            Option<&Vec<BlindedMessage>>,
            Option<&Vec<BlindSignature>>,
            Option<&ProofsExtended>,
        ) -> bool,
    ) -> Result<(), Error> {
        if mi.is_none() {
            let lock = self.counter.as_ref().unwrap().lock().await;
            mi = lock.mnemonic.clone();
        }
        let mi = mi.unwrap();

        let mut life = vec![];
        let keysetids = if keysetids.is_empty() {
            let keysets = self.client.get_keysetids().await?.keysets;
            keysets
                .iter()
                .filter(|ks| ks.id.version != KeySetVersion::VersionBs)
                .for_each(|k| life.push(k.id.to_string()));
            &life[..]
        } else {
            keysetids
        };

        for (idx, ki) in keysetids.iter().enumerate() {
            let keysetid = ki.to_string();

            let keys = self.client.get_keys(Some(&keysetid)).await?;
            if keys.keysets.is_empty() {
                continue;
            }

            let keysets = &keys.keysets[..];
            let keyset = &keysets[0];

            let mut manager = Manager::new(&self.client().url)
                .mnemonic(Some(mi.clone()))
                .records(vec![], keysets);
            let mut counter = manager.start_count(Some(keyset.unit.as_str()), keysets)?;

            let mut offset = 0u64;
            let mut emptys = 0usize;
            while emptys < 3 {
                let mut outputs = PreMintSecretsHyper::split_blanks(batch_size, &mut counter)?;
                let blinds = BlindedMessages::new(&outputs.secrets);
                f(
                    self.client().url().as_str(),
                    keysetids.len(),
                    idx,
                    &keysetid,
                    keyset.unit.as_str(),
                    counter.before(),
                    batch_size,
                    counter.now(),
                    Some(&outputs.secrets),
                    None,
                    None,
                    None,
                );

                // #[rustfmt::skip]
                // info!("{}~{}-{}: gen outputs {}:\n{}", counter.before(), counter.now(), batch_size, outputs.len(), serde_json::to_string(&blinds).unwrap());

                let resp = self.client.restore(&blinds).await?;
                let signatures = if resp.signatures.is_empty() {
                    resp.promises
                } else {
                    resp.signatures
                };

                f(
                    self.client().url().as_str(),
                    keysetids.len(),
                    idx,
                    &keysetid,
                    keyset.unit.as_str(),
                    counter.before(),
                    batch_size,
                    counter.now(),
                    Some(&outputs.secrets),
                    Some(&resp.outputs),
                    Some(&signatures),
                    None,
                );

                if !resp.outputs.is_empty() {
                    let last = resp.outputs.last().unwrap();
                    let lastidx = outputs
                        .secrets
                        .iter()
                        .rposition(|it| it.blinded_message == *last)
                        .map(|p| p + 1)
                        .unwrap_or(outputs.secrets.len()) as u64;

                    offset = counter.before() + lastidx;
                }

                outputs
                    .secrets
                    .retain(|x| resp.outputs.contains(&x.blinded_message));

                // #[rustfmt::skip]
                // info!("{}~{}-{}: got outputs {}:\n{}", counter.before(), counter.now(), batch_size, resp.outputs.len(), serde_json::to_string(&resp.outputs).unwrap());
                // #[rustfmt::skip]
                // info!("{}~{}-{}: got signatures {}:\n{}", counter.before(), counter.now(), batch_size, signatures.len(), serde_json::to_string(&signatures).unwrap());

                let ps = process_swap_response::<ProofExtended>(outputs, signatures, &keyset.keys)?;
                let states = self.check_proofs(&ps).await?.states;
                if states.len() != ps.len() {
                    return Err(Error::Custom(format_err!(
                        "check_proofs mint retures states size unexpected"
                    )));
                }

                let ps = ps
                    .into_iter()
                    .zip(states.iter())
                    .filter(|(_, s)| s.state != State::Spent)
                    .map(|ps| ps.0)
                    .collect::<Vec<_>>();

                // for log, etcs
                let exit = f(
                    self.client().url().as_str(),
                    keysetids.len(),
                    idx,
                    &keysetid,
                    keyset.unit.as_str(),
                    counter.before(),
                    batch_size,
                    counter.now(),
                    None,
                    None,
                    None,
                    Some(&ps),
                );
                ps.into_iter().for_each(|mut p| {
                    p.unit = Some(keyset.unit.as_str().to_owned());
                    proofs.push(p)
                });

                // let token = self.proofs_to_token(&proofs, None, Some(keyset.unit.as_str()))?;
                // println!("{}", token);

                if exit {
                    return Ok(());
                }

                // only for next batch restore
                counter.commit(()).await.unwrap();

                tokio::time::sleep(std::time::Duration::from_millis(
                    sleepms_after_check_a_batch,
                ))
                .await;

                if resp.outputs.is_empty() {
                    emptys += 1;
                }
            }

            if offset > 0 {
                let mut record = Record::new(
                    self.client().url().as_str(),
                    keysetid,
                    Some(mi.pubkey().to_owned()),
                );
                record.counter = offset;
                store
                    .add_record(&record)
                    .await
                    .map_err(|e| Error::Custom(e.into()))?;
            }
        }

        Ok(())
    }
}

// for auto fix count for mnemonic
async fn try_to_call_swap<'s, 'l: 's>(
    client: &'s MintClient,
    proofs: impl ProofsHelper + Copy,
    keep: Amount,
    send: Amount,
    denomination: Amount,
    counter: &'s mut ManagerCounter<'l>,
    store: impl RecordStore,
) -> Result<(PreMintSecretsHyper, SwapResponse), Error> {
    for i in (0..3).rev() {
        let outputs = PreMintSecretsHyper::split_amount2(keep, send, denomination, counter)?;
        let blinds = BlindedMessages::new(&outputs.messages.secrets);
        let swap_response = client.swap(proofs, &blinds).await;

        if counter.mnemonic().is_some() {
            let mut b = false;
            match &swap_response {
                Ok(_r) => {}
                Err(e) => {
                    b = e.is_outputs_already_signed_before();
                    if b && i > 0 {
                        continue;
                    }
                }
            }

            // simple auto fix stale store
            if swap_response.is_ok() || b {
                counter.commit(store).await?;
            }
        }
        return swap_response.map(|s| (outputs, s)).map_err(|e| e.into());
    }
    unreachable!()
}

pub type SplitProofs = SplitProofsGeneric<Proof>;
pub type SplitProofsExtended = SplitProofsGeneric<ProofExtended>;

/// Wrap for split token
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitProofsGeneric<P: AsRef<Proof>> {
    pub(crate) proofs: Vec<P>,
    pub(crate) send_idx_start: usize,
}

impl<P: AsRef<Proof>> SplitProofsGeneric<P> {
    pub fn new(proofs: Vec<P>, send_idx_start: usize) -> Self {
        assert!(send_idx_start <= proofs.len());

        Self {
            proofs,
            send_idx_start,
        }
    }

    pub fn keep(&self) -> &[P] {
        &self.proofs[..self.send_idx_start]
    }

    pub fn send(&self) -> &[P] {
        &self.proofs[self.send_idx_start..]
    }
    pub fn all(&self) -> &[P] {
        &self.proofs
    }
    pub fn into_inner(self) -> (Vec<P>, usize) {
        (self.proofs, self.send_idx_start)
    }
}

/// Wrap for generate output BlindedMessages
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PreMintSecretsHyper {
    pub messages: PreMintSecrets,
    pub(crate) send_idx_start: usize,
}

impl PreMintSecretsHyper {
    pub fn new(messages: PreMintSecrets, send_idx_start: usize) -> Self {
        assert!(send_idx_start <= messages.secrets.len());

        Self {
            messages,
            send_idx_start,
        }
    }

    pub fn send_idx_start(&self) -> usize {
        self.send_idx_start
    }

    /// Blank Outputs used for NUT-08 change https://github.com/cashubtc/nuts/blob/main/08.md
    pub fn split_blank(
        fee_reserve: Amount,
        counter: &mut ManagerCounter,
    ) -> Result<PreMintSecrets, Error> {
        let count = ((u64::from(fee_reserve) as f64).log2().ceil() as u64).max(1);
        Self::split_blanks(count, counter)
    }

    pub fn split_blanks(count: u64, counter: &mut ManagerCounter) -> Result<PreMintSecrets, Error> {
        let mut secrets = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let c = counter.count();
            let p = counter.generate(c, Amount::ZERO)?;
            secrets.push(p);
        }

        Ok(PreMintSecrets { secrets })
    }

    // mint token
    pub fn split_amount(
        amount: Amount,
        counter: &mut ManagerCounter,
    ) -> Result<PreMintSecrets, Error> {
        Self::split_amount2(amount, 0.into(), 0.into(), counter).map(|s| s.messages)
    }

    // send
    /// Create BlindedMessages with amount and denomination(used to split send, default is 0, meaning using random spilt)
    pub fn split_amount2(
        amount_keep: Amount,
        amount_send: Amount,
        denomination: Amount,
        counter: &mut ManagerCounter,
    ) -> Result<Self, Error> {
        let splited_keep = amount_keep.split();
        let splited_keep_len = splited_keep.len();

        let uni = denomination.to_u64();
        let splited_send = if amount_send == Amount::ZERO {
            vec![]
        } else if uni == 0 {
            amount_send.split()
        } else {
            let send = amount_send.to_u64();

            if uni > 2 {
                // maybe todo
                panic!("support 2^n only(now n <= 1): {}", uni);
            }

            let units = send / uni;
            let others = send % uni;
            let others = Amount::from(others).split();

            let mut sp = Vec::with_capacity(units as usize + others.len());
            for _ in 0..units {
                sp.push(denomination);
            }
            for a in others {
                sp.push(a);
            }

            sp
        };

        let capacity = splited_keep.len() + splited_send.len();
        let mut secrets = Vec::with_capacity(capacity);
        for amount in splited_keep.into_iter().chain(splited_send.into_iter()) {
            let c = counter.count();
            let p = counter.generate(c, amount)?;
            secrets.push(p);
        }

        let messages = PreMintSecrets { secrets };

        Ok(Self::new(messages, splited_keep_len))
    }
}

/// generate Proofs from swaps response
pub fn process_swap_response<P: From<Proof>>(
    pre_secrets: PreMintSecrets,
    promises: Vec<BlindSignature>,
    keys: &Keys,
) -> Result<Vec<P>, Error> {
    let pre_secrets = pre_secrets.secrets;
    if pre_secrets.len() < promises.len() {
        Err(Error::Custom(format_err!(
            "promises size unexpected: promises: {}, pre_secrets: {}",
            promises.len(),
            pre_secrets.len(),
        )))?;
    }

    let mut proofs = Vec::with_capacity(promises.len());

    for (promise, pre_secret) in promises.into_iter().zip(pre_secrets.into_iter()) {
        let a = keys
            .amount_key(promise.amount)
            .ok_or_else(|| format_err!("not found amount key: {}", promise.amount.to_u64()))?
            .to_owned();

        let r = pre_secret.r;
        let c = unblind_message(&promise.c, &r, &a)?;

        let proof = Proof {
            keyset_id: promise.keyset_id,
            amount: promise.amount,
            secret: pre_secret.secret,
            dleq: promise
                .dleq
                .map(|BlindSignatureDleq { e, s }| ProofDleq { e, s, r }),
            witness: None,
            c,
        };

        proofs.push(proof.into());
    }

    Ok(proofs)
}

#[cfg(test)]
mod tests {
    use crate::wallet::client::HttpOptions;

    use super::*;

    #[allow(non_upper_case_globals)]
    const mint_url: &str = "https://8333.space:3338/";

    // cargo t wallet::tests --  --nocapture

    async fn new() -> Result<Wallet, Error> {
        let c = HttpOptions::new()
            .connection_verbose(true)
            .timeout_connect_ms(3000)
            .timeout_swap_ms(5000);
        let client = MintClient::new(mint_url.parse().unwrap(), c)?;

        let w = Wallet::new(client, None, None, None, ()).await?;
        let keys = w.keyset0(None).await.unwrap().keys.keys();
        #[rustfmt::skip]
        println!("keyset {}: {:?}", keys.len(), keys.keys().collect::<Vec<_>>());
        assert_eq!(keys.len(), 64);

        Ok(w)
    }

    #[tokio::test]
    async fn test_new() {
        let w = new().await.unwrap();
        println!("url: {}", w.client().url());
        assert_eq!(w.client().url().as_str(), mint_url);

        // 400: {"detail":"Token already spent.","code":11001}
        let token = "cashuAeyJ0b2tlbiI6W3sibWludCI6Imh0dHBzOi8vODMzMy5zcGFjZTozMzM4IiwicHJvb2ZzIjpbeyJhbW91bnQiOjEsInNlY3JldCI6Iis2cmo4amIyR0FHUGVWQWJNM0tHeEg5OVQ3ek9VR1VxalQ2S2hLUUw0bUk9IiwiQyI6IjAzYTFlNDg2NmI3ZDU4MGM1ZjA3YjNkYzcyMmUwMWEyY2IyNDBkNjVmMjg5ZTQxYWE2Yjc3YmNkNjE1ZDA1ZDJlYiIsImlkIjoiSTJ5TitpUllma3pUIn0seyJhbW91bnQiOjEsInNlY3JldCI6Ikc5blhPSG1GQVBsYWQ0OWhPZTFlZXRscWkyZ2RkUFkyWk52aWxQZ3JPeXM9IiwiQyI6IjAyYjUwNGQ0MjVkYjlhY2ZhZjYxZjUwZGU0ZjUzNDQwMDliYWM2OWI4NDA0YThmMTY3OTg3YjA1MTA5NzhlNDE0ZSIsImlkIjoiSTJ5TitpUllma3pUIn0seyJhbW91bnQiOjEsInNlY3JldCI6Ik9MS21CaUhoTEZRV1Nmbm5NZENPdHdIV2FoWStNWFNJKytNNzJpbGVUOXM9IiwiQyI6IjAzN2NmZDhiMjRlZWRmZTM4ZDk2YmRkYzlhNGVmNGNjNDU1NmJkNWVjNWQ2ZjlhMzQ0ZTY5MWI2M2EzMjZjNmRmYiIsImlkIjoiSTJ5TitpUllma3pUIn0seyJhbW91bnQiOjEsInNlY3JldCI6IlFTcklLNllQZko3a3d5V3lLaEF2TjUwL2pmM1pKd0VlcGVpUGtLTE42WE09IiwiQyI6IjAzZGEzOGYzMDNiMmVlNDY4NTBkMzRlYzkwYWNkZDQyMzViZGFhNmM4YzMzZDJkMDRlNzNiMDQyMzU0MTBjZjhiNiIsImlkIjoiSTJ5TitpUllma3pUIn0seyJhbW91bnQiOjEsInNlY3JldCI6ImdmYmRWakMyZHRJbkpacGFHc0VPQmZ2dzNIQVFldzZqcGM1YVhETUIxVDQ9IiwiQyI6IjAyNDExN2UxNzhjZmE3Y2Y1OTE4NTBlYjI0NzNhN2E2N2Q0NTUxZTAyZjY3ZjQ1MDAxYmUyNzI2MWE1ZTNiZmMwYiIsImlkIjoiSTJ5TitpUllma3pUIn0seyJhbW91bnQiOjEsInNlY3JldCI6Im1MK2g4eWVKbVI1VCt1MFhOb1NWbzRpSWcrcTMrQ283bmhxZVVtN1JkQ009IiwiQyI6IjAyZDEzNWFlMDliNGVkMzg0ZmI4MTJjNjc3NDNlYjcxODhmZjgzNWUwNjJkY2YzMDEzM2UzNDc0NTUwYzk3ZWExNiIsImlkIjoiSTJ5TitpUllma3pUIn0seyJhbW91bnQiOjEsInNlY3JldCI6InNHMEVBVXdmWHhjWm9NajNjY3hYRzg1eVNOWTBNQW10MXVZYm9ZRkptM2M9IiwiQyI6IjAyZmMxODNlNzg0MTAyZDNiMjg5YjJiZjJmNzkzZTE0MzkzMTI2YWZhYWQxZGFiZjU1Nzk2NmZkM2EyYTEzNGJmNSIsImlkIjoiSTJ5TitpUllma3pUIn0seyJhbW91bnQiOjEsInNlY3JldCI6InlFTDdzT2tVRnh3bUVUNXhDbk5hT2ZoeEFJNmF1TVVLUmZjRmZyVlhvcVE9IiwiQyI6IjAyNWM4ZDQ4NTg5NjBlMWVlNzQyZjFhZmI3N2YwZWYyY2FmYWYxM2ZmOTEyM2I0NWYwYTJmYzEwODEzYmE2ZjhmNSIsImlkIjoiSTJ5TitpUllma3pUIn1dfV0sIm1lbW8iOm51bGx9";
        let t: Token = token.trim().parse().unwrap();
        let mut ps = vec![];
        let r = w.receive(&t, &mut ps, ()).await.unwrap_err();
        println!("receive spent {}: {:?}", ps.len(), r);
        assert_eq!(ps.len(), 0);
    }
}
