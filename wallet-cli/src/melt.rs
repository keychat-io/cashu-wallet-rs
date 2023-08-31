use crate::opts::MeltOpts as Opts;

use cashu_wallet::cashu::nuts::MeltQuoteBolt11Response;
use cashu_wallet::store::UnitedStore;
use cashu_wallet::{UniError, UniErrorFrom, UnitedWallet};

impl Opts {
    pub async fn run<S>(self, wallet: UnitedWallet<S>)
    where
        S: UnitedStore + Clone + Send + Sync + 'static,
        UniError<S::Error>: UniErrorFrom<S>,
    {
        let mut quote_response = Some(MeltQuoteBolt11Response {
            quote: String::new(),
            amount: 0,
            fee_reserve: 0,
            expiry: 0,
            paid: false,
        });

        let res = self.fun(wallet, quote_response.as_mut()).await;
        if let Some(qr) = quote_response {
            if qr.amount > 0 {
                println!("{:?}", qr)
            }
        }

        match res {
            Ok(_) => {}
            Err(e) => {
                error!("run failed: {}", e);
                std::process::exit(1);
            }
        }
    }

    async fn fun<S>(
        &self,
        wallet: UnitedWallet<S>,
        qr: Option<&mut MeltQuoteBolt11Response>,
    ) -> Result<(), UniError<S::Error>>
    where
        S: UnitedStore + Clone + Send + Sync + 'static,
        UniError<S::Error>: UniErrorFrom<S>,
    {
        // let _mints = wallet.load_mints_from_database().await?;
        let mint_url: cashu_wallet::Url = self.mint.parse()?;
        wallet.add_mint(mint_url.clone(), false).await?;

        let tx = wallet
            .melt(
                &mint_url,
                self.request.clone(),
                None,
                Some(self.unit.as_str()),
                qr,
            )
            .await?;
        info!("{:?}", tx);
        Ok(())
    }
}
