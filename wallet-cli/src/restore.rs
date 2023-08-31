use std::sync::Arc;

use cashu_wallet::store::{ProofsExtended, UnitedStore};
use cashu_wallet::wallet::{AmountHelper, ProofsHelper};
use cashu_wallet::{UniError, UniErrorFrom, UnitedWallet};

use crate::opts::RestoreOpts as Opts;

impl Opts {
    pub async fn run<S>(self, wallet: UnitedWallet<S>)
    where
        S: UnitedStore + Clone + Send + Sync + 'static,
        UniError<S::Error>: UniErrorFrom<S>,
    {
        match self.fun(wallet).await {
            Ok(_) => {}
            Err(e) => {
                error!("run failed: {}", e);
                std::process::exit(1);
            }
        }
    }

    async fn fun<S>(&self, wallet: UnitedWallet<S>) -> Result<(), UniError<S::Error>>
    where
        S: UnitedStore + Clone + Send + Sync + 'static,
        UniError<S::Error>: UniErrorFrom<S>,
    {
        // let _mints = wallet.load_mints_from_database().await?;

        let mint_url: cashu_wallet::Url = self.mint.parse()?;
        wallet.add_mint(mint_url.clone(), false).await?;

        let mut keysetids = vec![];
        if !self.keysetid.is_empty() {
            keysetids.push(self.keysetid.clone());
        }

        let mut mnemonic = None;
        if !self.words.is_empty() {
            use cashu_wallet::wallet::MnemonicInfo;

            let mi = MnemonicInfo::with_words(&self.words)?;
            mnemonic = Some(Arc::new(mi));
        }

        use cashu_wallet::cashu::nuts::nut00;
        let f = |mint: &str,
                 keysets: usize,
                 keysetidx: usize,
                 keysetid: &str,
                 unit: &str,
                 before: u64,
                 batch: u64,
                 now: u64,
                 secrets: Option<&Vec<nut00::PreMint>>,
                 blinds: Option<&Vec<nut00::BlindedMessage>>,
                 signatures: Option<&Vec<nut00::BlindSignature>>,
                 proofs: Option<&ProofsExtended>| {
            info!(
                "{} {}/{} {} {} {}:{}:{} gen premints {}, got blinds: {}, got signatures {}, coins: {}, value: {}",
                mint,
                keysets,
                keysetidx,
                keysetid,
                unit,
                before,
                batch,
                now,
                secrets.map(|x| x.len()).unwrap_or(0),
                blinds.map(|x| x.len()).unwrap_or(0),
                signatures.map(|x| x.len()).unwrap_or(0),
                proofs.as_ref().map(|x| x.len()).unwrap_or(0),
                proofs.as_ref().map(|x| x.sum().to_u64()).unwrap_or(0),
            );

            false
        };

        let ps = wallet
            .restore(&mint_url, self.batch, self.sleepms, &keysetids, mnemonic, f)
            .await?;

        info!("restore: {} coins", ps.len());
        let mut coins = std::collections::BTreeMap::new();

        for p in ps {
            let entry = coins
                .entry(p.unit().unwrap_or_default().to_string())
                .or_insert(vec![]);

            entry.push(p);
        }

        for (c, ps) in coins {
            println!("{}: coins: {}, value: {}", c, ps.len(), ps.sum());
        }

        Ok(())
    }
}
