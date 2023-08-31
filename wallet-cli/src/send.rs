use crate::opts::SendOpts as Opts;

use cashu_wallet::store::{ProofExtended, UnitedStore};
use cashu_wallet::wallet::{AmountHelper, ProofsHelper};
use cashu_wallet::{UniError, UniErrorFrom, UnitedWallet, Url};

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
        let mint_url: Url = self.mint.parse()?;
        // wallet.load_mints_from_database().await?;
        wallet.add_mint(mint_url.clone(), false).await?;

        let mut amount = self.value;
        let unit = self.unit.as_str();
        let mut ps = wallet
            .store()
            .get_proofs_limit_unit(&mint_url, unit)
            .await?;
        if amount == 0 {
            amount = ps.sum().to_u64();
        }

        let select = cashu_wallet::select_send_proofs(amount, &mut ps)?;
        if self.limit > 0 && select as u64 + 1 > self.limit {
            warn!(
                "merge proofs, not exit!!!: {}/{} proofs > {}",
                select + 1,
                ps.len(),
                self.limit
            );
            let (now, past) =
                merge_proofs_in_database(&wallet, &mint_url, self.limit, Some(unit), ps).await?;
            warn!("merge proofs ok: {}->{}", past, now);
        }

        let tx = wallet
            .send_tokens(&mint_url, amount, None, Some(unit), None)
            .await?;
        info!("send {} {}: {}", tx.amount(), tx.status().as_ref(), tx.id());
        println!("{}", tx.content());

        Ok(())
    }
}

pub async fn merge_proofs_in_database<S>(
    this: &UnitedWallet<S>,
    mint_url: &cashu_wallet::Url,
    limit: u64,
    unit: Option<&str>,
    mut proofs: Vec<ProofExtended>,
) -> Result<(usize, usize), UniError<S::Error>>
where
    S: UnitedStore + Clone + Send + Sync + 'static,
    UniError<S::Error>: UniErrorFrom<S>,
{
    let size0 = proofs.len();
    let w = this.get_wallet(mint_url)?;

    let batch_size = 64;
    loop {
        // proofs.retain(|p| p.amount.to_sat() < 100);

        let size0 = proofs.len();
        let times = (size0 as f64 / batch_size as f64).ceil();
        for (idx, chunk) in proofs
            .chunks(batch_size)
            .filter(|c| c.len() > 5)
            .enumerate()
        {
            let a = chunk.sum();
            let got = w.send(a.into(), chunk, unit, this.store()).await?;

            info!(
                "merge proofs {}/{}: {}->{}",
                idx,
                times,
                chunk.len(),
                got.all().len()
            );

            this.store().add_proofs(mint_url, got.all()).await?;
            this.store().delete_proofs(mint_url, chunk).await?;
        }

        proofs = this
            .store()
            .get_proofs_limit_unit(mint_url, unit.unwrap())
            .await?;

        if proofs.len() <= limit as usize || proofs.len() == size0 {
            break;
        }
    }

    Ok((proofs.len(), size0))
}
