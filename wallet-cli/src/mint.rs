use cashu_wallet::store::UnitedStore;
use cashu_wallet::{UniError, UniErrorFrom, UnitedWallet};

use crate::opts::MintOpts as Opts;

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

        let tx = wallet
            .request_mint(&mint_url, self.value, Some(self.unit.as_str()))
            .await?;
        info!("{:?}", tx);
        Ok(())
    }
}
