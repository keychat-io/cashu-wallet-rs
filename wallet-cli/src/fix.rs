use crate::opts::FixOpts as Opts;

use cashu_wallet::store::UnitedStore;
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
        wallet.load_mints_from_database().await?;

        let res = wallet.check_proofs_in_database().await?;
        if res.0 == 0 {
            info!("run ok: {:?}", res);
        } else {
            error!("run ok: {:?}", res);
        }

        Ok(())
    }
}
