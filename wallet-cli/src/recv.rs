use crate::opts::RecvOpts as Opts;

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
        // let tokens: cashu_wallet::wallet::Token = self.tokens[0].parse().unwrap();
        let _mints = wallet
            .load_mints_from_database()
            .await
            .map_err(|e| error!("load_mints_from_database failed: {}", e));

        let mint_url: cashu_wallet::Url = self.mint.parse()?;
        wallet.add_mint(mint_url, false).await?;

        for (i, token) in self.tokens.iter().enumerate() {
            let prefix = "cashuA";
            if token.starts_with(prefix) {
                let token = &token[prefix.len()..];

                use base64::{alphabet, engine::general_purpose, Engine};

                let decode_config = general_purpose::GeneralPurposeConfig::new()
                    .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent);
                let decoded =
                    general_purpose::GeneralPurpose::new(&alphabet::STANDARD, decode_config)
                        .decode(token)
                        .map_err(|e| UniError::Custom(e.into()))?;

                let js = String::from_utf8(decoded).map_err(|e| UniError::Custom(e.into()))?;

                println!("{}", js);
            }

            if self.percoin {
                use cashu_wallet::wallet::MintProofs;
                use cashu_wallet::wallet::Token;
                use cashu_wallet::wallet::TokenV3;

                let tokens: Token = token.parse()?;
                let tokens = tokens.into_v3()?;
                let mut count = (0, 0, 0);
                for (i2, t2) in tokens.token.into_iter().enumerate() {
                    // reorder for Rate limit exceeded.
                    let mint = wallet.get_wallet(&t2.mint)?;
                    let states = mint.check_proofs(&t2.proofs).await?.states;
                    use cashu_wallet::cashu::nuts::State;
                    let unspents = states
                        .iter()
                        .enumerate()
                        .filter(|p| p.1.state == State::Unspent)
                        .map(|p| p.0)
                        .collect::<Vec<_>>();

                    let proofs0 = t2
                        .proofs
                        .iter()
                        .enumerate()
                        .filter(|(idx, _)| unspents.contains(idx))
                        .map(|ip| ip.1);
                    let proofs1 = t2
                        .proofs
                        .iter()
                        .enumerate()
                        .filter(|(idx, _)| !unspents.contains(idx))
                        .map(|ip| ip.1);

                    for (i3, p) in proofs0.chain(proofs1).into_iter().enumerate() {
                        let gta = p.amount;
                        let mps = MintProofs {
                            mint: t2.mint.clone(),
                            proofs: vec![p.clone()],
                        };
                        let pt = TokenV3 {
                            token: vec![mps],
                            memo: tokens.memo.clone(),
                            unit: tokens.unit.clone(),
                        };
                        let gt = pt.to_string();

                        info!(
                            "{}.{}.{} {} {}: {}",
                            i,
                            i2,
                            i3,
                            gta.to_u64(),
                            pt.unit().unwrap_or_default(),
                            gt
                        );
                        match wallet.receive_tokens(&gt).await {
                            Ok(a) => {
                                count.0 += 1;
                                count.1 += a;
                                info!("{}.{}.{} recv ok: {}", i, i2, i3, a)
                            }
                            Err(e) => {
                                count.2 += 1;
                                error!("{}.{}.{} recv failed: {}", i, i2, i3, e)
                            }
                        }
                    }
                }
                info!("recv {} coins, ok {}, failed {}", count.1, count.0, count.2);
            } else {
                match wallet.receive_tokens(token).await {
                    Ok(a) => info!("{} recv ok: {}", i, a),
                    Err(e) => info!("{} recv failed: {}", i, e),
                }
            }
        }

        Ok(())
    }
}
