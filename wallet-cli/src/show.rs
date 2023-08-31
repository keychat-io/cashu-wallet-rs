use crate::opts::ShowOpts as Opts;

use cashu_wallet::store::UnitedStore;
use cashu_wallet::wallet::AmountHelper;
use cashu_wallet::{UniError, UniErrorFrom, UnitedWallet};

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
        let balances = wallet.get_balances().await?;
        if balances.is_empty() {
            warn!("empty balances: {:?}", balances);
        }
        for (i, (k, v)) in balances.iter().enumerate() {
            info!("{:>2} {} {}: {}", i, k.mint(), k.unit(), v);
        }

        if self.check {
            wallet.load_mints_from_database().await?;

            let (upc, pc) = wallet.check_pendings().await?;
            warn!("check_pendings ok: {}/{}", upc, pc);

            if upc > 0 {
                let balances = wallet.get_balances().await?;
                for (i, (k, v)) in balances.iter().enumerate() {
                    info!("{:>2} {} {}: {}", i, k.mint(), k.unit(), v);
                }
            }
        }

        if self.transactions {
            let mut pendings = vec![];

            let mut txs = wallet.store().get_all_transactions().await?;
            txs.sort_by(|a, b| a.time().cmp(&b.time()));

            let txs_is_success = txs.iter().filter(|tx| tx.status().is_success()).count();
            let txs_is_failed = txs
                .iter()
                .filter(|tx| tx.status().is_failed() || tx.status().is_expired())
                .count();
            let txs_is_pending = txs.iter().filter(|tx| tx.status().is_pending()).count();
            info!(
                "get_all_transactions len: {} ok: {}, failed: {}, pending: {}",
                txs.len(),
                txs_is_success,
                txs_is_failed,
                txs_is_pending
            );

            txs.sort_by_key(|a| a.time());

            let skip = if self.limit > 0 && txs.len() > self.limit {
                txs.len() - self.limit
            } else {
                0
            };

            for (idx, tx) in txs.iter().enumerate().skip(skip) {
                println!(
                    "{:>2} {}: {:>3} {:>7} {} {} {} {}",
                    idx,
                    tx.time(),
                    tx.direction().as_ref(),
                    tx.status().as_ref(),
                    tx.unit().unwrap_or_default(),
                    tx.amount(),
                    tx.id(),
                    tx.mint_url(),
                );

                // *tx.status_mut() = cashu_wallet::types::TransactionStatus::Pending;
                // w.store().add_transaction(&tx).await.unwrap();
            }

            for (_idx, tx) in txs.into_iter().enumerate() {
                if tx.is_pending() {
                    pendings.push(tx.clone());
                }
            }

            for (i, tx) in pendings.into_iter().enumerate() {
                info!(
                    "{:>2} {} {} {}: {}",
                    tx.time(),
                    i,
                    tx.amount(),
                    tx.unit().unwrap_or_default(),
                    tx.content()
                );

                if self.check && self.recycle && tx.is_cashu() {
                    let res = wallet.receive_tokens(tx.content()).await;
                    info!("{:>2} {} recv {}: {:?}", i, tx.content(), tx.amount(), res);
                }
            }
        }

        if self.proofs {
            let ps = wallet.store().get_all_proofs().await?;
            info!("get_all_proofs len: {:?}", ps.len());

            for (k, v) in ps {
                info!("get_proofs_{} {} len: {:?}", k.mint(), k.unit(), v.len());

                let skip = if self.limit > 0 && v.len() > self.limit {
                    v.len() - self.limit
                } else {
                    0
                };

                for (idx, p) in v.into_iter().enumerate().skip(skip) {
                    let pr = &p.raw;
                    println!(
                        "{:>2} {} {}: {} {}",
                        idx,
                        p.ts.and_then(|t| t.try_into().ok()).unwrap_or(-1i128),
                        pr.amount.to_u64(),
                        pr.keyset_id,
                        pr.secret.as_str(),
                    );
                }
            }
        }

        Ok(())
    }
}
