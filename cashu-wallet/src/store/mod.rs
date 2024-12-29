use std::error::Error as StdError;

use std::collections::BTreeMap as Map;

pub use crate::wallet::{MintUrl as Url, Proof, ProofExtended, Proofs, ProofsExtended, Record};

use crate::types::Mint;
use crate::types::Transaction;
use crate::types::TransactionKind;
use crate::types::TransactionStatus;

pub type MintUrlWithUnitOwned = MintUrlWithUnit<'static>;

use std::borrow::Cow;
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub struct MintUrlWithUnit<'a> {
    mint: Cow<'a, str>,
    unit: Cow<'a, str>,
}

impl<'a> MintUrlWithUnit<'a> {
    pub fn new(mint_url: impl Into<Cow<'a, str>>, unit: impl Into<Cow<'a, str>>) -> Self {
        Self {
            mint: mint_url.into(),
            unit: unit.into(),
        }
    }
    pub fn into_owned(self) -> MintUrlWithUnitOwned {
        MintUrlWithUnit {
            mint: self.mint.into_owned().into(),
            unit: self.unit.into_owned().into(),
        }
    }
    pub fn mint(&self) -> &str {
        self.mint.as_ref()
    }
    pub fn unit(&self) -> &str {
        self.unit.as_ref()
    }
}

use std::cmp::Ord;
use std::cmp::Ordering;
#[doc(hidden)]
pub fn cmp_by_asc<T: Ord>(a: T, b: T) -> Ordering {
    a.cmp(&b)
}

#[test]
fn test_cmp_by() {
    let mut ps = vec![Some(1), None, Some(10), Some(7), None];

    let asc = vec![None, None, Some(1), Some(7), Some(10)];
    let desc = asc.clone().into_iter().rev().collect::<Vec<_>>();

    ps.sort_by(|a, b| cmp_by_asc(a, b));
    assert_eq!(ps, asc);

    ps.sort_by(|a, b| cmp_by_asc(b, a));
    assert_eq!(ps, desc);
}

/// multiple mints wallet store
#[async_trait]
pub trait UnitedStore {
    type Error: StdError + Send + Sync;

    // counter records
    async fn add_counter(&self, record: &Record) -> Result<(), Self::Error>;
    async fn delete_counters(&self, mint_url: &Url) -> Result<(), Self::Error>;
    async fn get_counters(&self, mint_url: &Url, pubkey: &str) -> Result<Vec<Record>, Self::Error>;
    // async fn get_all_counters(&self) -> Result<Map<String, Vec<Record>>, Self::Error>;
    // proofs
    async fn delete_proofs(
        &self,
        mint_url: &Url,
        proofs: &[ProofExtended],
    ) -> Result<(), Self::Error>;
    async fn add_proofs(&self, mint_url: &Url, proofs: &[ProofExtended])
        -> Result<(), Self::Error>;
    async fn get_proofs_limit_unit(
        &self,
        mint_url: &Url,
        unit: &str,
    ) -> Result<ProofsExtended, Self::Error>;
    async fn get_proofs(&self, mint_url: &Url) -> Result<Map<String, ProofsExtended>, Self::Error>;
    async fn get_all_proofs(
        &self,
    ) -> Result<Map<MintUrlWithUnitOwned, ProofsExtended>, Self::Error>;
    //
    async fn migrate(&self) -> Result<(), Self::Error>;
    //
    // mints
    async fn add_mint(&self, mint: &Mint) -> Result<(), Self::Error>;
    async fn get_mint(&self, mint_url: &str) -> Result<Option<Mint>, Self::Error>;
    async fn get_mints(&self) -> Result<Vec<Mint>, Self::Error>;
    //
    // tx
    async fn delete_transactions(
        &self,
        status: &[TransactionStatus],
        unix_timestamp_ms_le: u64,
    ) -> Result<u64, Self::Error>;
    async fn add_transaction(&self, tx: &Transaction) -> Result<(), Self::Error>;
    async fn get_transaction(&self, txid: &str) -> Result<Option<Transaction>, Self::Error>;
    async fn get_transactions(
        &self,
        status: &[TransactionStatus],
    ) -> Result<Vec<Transaction>, Self::Error>;
    async fn get_pending_transactions(&self) -> Result<Vec<Transaction>, Self::Error> {
        self.get_transactions([TransactionStatus::Pending].as_slice())
            .await
    }
    async fn get_all_transactions(&self) -> Result<Vec<Transaction>, Self::Error> {
        self.get_transactions(
            [
                TransactionStatus::Pending,
                TransactionStatus::Success,
                TransactionStatus::Failed,
                TransactionStatus::Expired,
            ]
            .as_slice(),
        )
        .await
    }
    async fn get_transactions_with_offset(
        &self,
        offset: usize,
        limit: usize,
        kinds: &[TransactionKind],
    ) -> Result<Vec<Transaction>, Self::Error> {
        let mut txs = self
            .get_transactions(
                [
                    TransactionStatus::Pending,
                    TransactionStatus::Success,
                    TransactionStatus::Failed,
                    TransactionStatus::Expired,
                ]
                .as_slice(),
            )
            .await?;
        txs.retain(|tx| kinds.contains(&tx.kind()));

        let out = vec![];
        if txs.is_empty() || offset + 1 > txs.len() {
            return Ok(out);
        }

        txs.sort_by(|a, b| cmp_by_asc(b.time(), a.time()));

        let remains = &txs[offset..];
        let take = std::cmp::min(remains.len(), limit);

        Ok(remains[..take].to_vec())
    }
}

#[async_trait]
impl<T> UnitedStore for std::sync::Arc<T>
where
    T: UnitedStore + Sync + Send,
{
    type Error = T::Error;
    // counter records
    async fn add_counter(&self, records: &Record) -> Result<(), Self::Error> {
        self.as_ref().add_counter(records).await
    }
    async fn delete_counters(&self, mint_url: &Url) -> Result<(), Self::Error> {
        self.as_ref().delete_counters(mint_url).await
    }
    async fn get_counters(&self, mint_url: &Url, pubkey: &str) -> Result<Vec<Record>, Self::Error> {
        self.as_ref().get_counters(mint_url, pubkey).await
    }
    async fn delete_proofs(
        &self,
        mint_url: &Url,
        proofs: &[ProofExtended],
    ) -> Result<(), Self::Error> {
        self.as_ref().delete_proofs(mint_url, proofs).await
    }
    async fn add_proofs(
        &self,
        mint_url: &Url,
        proofs: &[ProofExtended],
    ) -> Result<(), Self::Error> {
        self.as_ref().add_proofs(mint_url, proofs).await
    }
    async fn get_proofs(&self, mint_url: &Url) -> Result<Map<String, ProofsExtended>, Self::Error> {
        self.as_ref().get_proofs(mint_url).await
    }
    async fn get_proofs_limit_unit(
        &self,
        mint_url: &Url,
        unit: &str,
    ) -> Result<ProofsExtended, Self::Error> {
        self.as_ref().get_proofs_limit_unit(mint_url, unit).await
    }
    async fn get_all_proofs(
        &self,
    ) -> Result<Map<MintUrlWithUnitOwned, ProofsExtended>, Self::Error> {
        self.as_ref().get_all_proofs().await
    }
    //
    async fn migrate(&self) -> Result<(), Self::Error> {
        self.as_ref().migrate().await
    }
    //
    // mints
    async fn add_mint(&self, mint: &Mint) -> Result<(), Self::Error> {
        self.as_ref().add_mint(mint).await
    }
    async fn get_mint(&self, mint_url: &str) -> Result<Option<Mint>, Self::Error> {
        self.as_ref().get_mint(mint_url).await
    }
    async fn get_mints(&self) -> Result<Vec<Mint>, Self::Error> {
        self.as_ref().get_mints().await
    }
    //
    // tx
    async fn delete_transactions(
        &self,
        status: &[TransactionStatus],
        unix_timestamp_ms_le: u64,
    ) -> Result<u64, Self::Error> {
        self.as_ref()
            .delete_transactions(status, unix_timestamp_ms_le)
            .await
    }
    async fn add_transaction(&self, tx: &Transaction) -> Result<(), Self::Error> {
        self.as_ref().add_transaction(tx).await
    }
    async fn get_transaction(&self, txid: &str) -> Result<Option<Transaction>, Self::Error> {
        self.as_ref().get_transaction(txid).await
    }
    async fn get_transactions(
        &self,
        status: &[TransactionStatus],
    ) -> Result<Vec<Transaction>, Self::Error> {
        self.as_ref().get_transactions(status).await
    }
    async fn get_pending_transactions(&self) -> Result<Vec<Transaction>, Self::Error> {
        self.as_ref().get_pending_transactions().await
    }
    async fn get_all_transactions(&self) -> Result<Vec<Transaction>, Self::Error> {
        self.as_ref().get_all_transactions().await
    }
    async fn get_transactions_with_offset(
        &self,
        offset: usize,
        limit: usize,
        kinds: &[TransactionKind],
    ) -> Result<Vec<Transaction>, Self::Error> {
        self.as_ref()
            .get_transactions_with_offset(offset, limit, kinds)
            .await
    }
}

use crate::wallet::RecordStore;
#[async_trait]
impl<T> RecordStore for &T
where
    T: UnitedStore + Sync + Send + 'static,
{
    type Error = T::Error;
    async fn add_record(&self, record: &Record) -> Result<(), Self::Error> {
        self.add_counter(record).await
    }
    async fn delete_records(&self, mint_url: &Url) -> Result<(), Self::Error> {
        self.delete_counters(mint_url).await
    }
    async fn get_records(&self, mint_url: &Url, pubkey: &str) -> Result<Vec<Record>, Self::Error> {
        self.get_counters(mint_url, pubkey).await
    }
}

// #[cfg(test)]
pub mod tests {
    use super::*;

    #[allow(unused_imports)]
    use crate::types::unixtime_ms;
    use crate::{
        types::tests::{MINT_URL, MINT_URL_TEST as MINT_URL2},
        wallet::MnemonicInfo,
    };
    use crate::{
        types::{CashuTransaction, LNTransaction, TransactionDirection},
        wallet::{AmountHelper, ProofsHelper, CURRENCY_UNIT_SAT},
    };

    pub fn tmpfi(f: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let tmpdir = tempfile::TempDir::new().unwrap();
        let tf = tmpdir.as_ref().join(f);
        println!("{}", tf.display());
        (tmpdir, tf)
    }

    pub async fn test_mint<S: UnitedStore>(store: &S) -> Result<(), S::Error> {
        let mint = Mint {
            url: MINT_URL.to_owned(),
            active: true,
            time: unixtime_ms(),
            info: None,
        };

        store.add_mint(&mint).await?;

        let m = store.get_mint(&mint.url).await?.expect("None");
        assert_eq!(m, mint);
        let ms = store.get_mints().await?;
        assert_eq!(ms.as_slice(), [mint.clone()].as_slice());

        let info = serde_json::from_str(crate::types::tests::INFO).unwrap();

        let mut mint2 = Mint {
            url: MINT_URL2.to_owned(),
            active: true,
            time: unixtime_ms(),
            info: Some(info),
        };

        store.add_mint(&mint2).await?;
        let m2 = store.get_mint(&mint2.url).await?.expect("None");
        assert_eq!(m2, mint2);
        assert_ne!(m, m2);
        let mut ms = store.get_mints().await?;
        ms.sort_by(|a, b| a.url.cmp(&b.url));
        let mut ms2 = [mint.clone(), mint2.clone()];
        ms2.sort_by(|a, b| a.url.cmp(&b.url));
        assert_eq!(ms.as_slice(), ms2.as_slice());

        mint2.active = false;
        store.add_mint(&mint2).await?;
        let m2 = store.get_mint(&mint2.url).await?.expect("None");
        assert_eq!(m2.active, false);

        let mut ms = store.get_mints().await?;
        ms.retain(|m| m.active);
        assert_eq!(ms.as_slice(), [mint.clone()].as_slice());

        Ok(())
    }

    use crate::wallet::Manager;
    use cashu::nuts::nut01::KeysResponse;
    use std::sync::Arc;
    pub async fn test_counter<S: UnitedStore + Send + Sync + 'static>(
        store: &S,
    ) -> Result<(), S::Error> {
        let mnemonic = MnemonicInfo::with_words(
            "rough ahead uncle sport arena urge orbit solid catch frequent table mushroom",
        )
        .unwrap();
        let mnemonic = Some(Arc::new(mnemonic));

        let keysets = r#"{"keysets": [
            {
                "id": "00759e3f8b06b36f",
                "unit": "sat",
                "keys": {
                    "1": "038a935c51c76c780ff9731cfbe9ab477f38346775809fa4c514340feabbec4b3a"
                }
            },
            {
                "id": "000f01df73ea149a",
                "unit": "sat",
                "keys": {
                    "1": "03ba786a2c0745f8c30e490288acd7a72dd53d65afd292ddefa326a4a3fa14c566"
                }
            },
            {
                "id": "00c074b96c7e2b0e",
                "unit": "usd",
                "keys": {
                    "1": "03ba786a2c0745f8c30e490288acd7a72dd53d65afd292ddefa326a4a3fa14c566"
                }
            }
            ]}"#
        .trim();
        let keysets_bk = serde_json::from_str::<KeysResponse>(keysets)
            .unwrap()
            .keysets;
        let keysets = keysets_bk.clone();
        let pubkey = mnemonic.as_ref().unwrap().pubkey().to_owned();
        println!("keysets: {:?}", keysets.len());
        println!("pubkey: {}", pubkey);
        let mint_url = &MINT_URL.parse::<Url>().unwrap();

        {
            let records = store.get_counters(&mint_url, &pubkey).await.unwrap();
            assert_eq!(0, records.len());

            let mut manager = Manager::new(&mint_url)
                .mnemonic(mnemonic.clone())
                .records(records, &keysets);
            let mut c = manager.start_count(None, &keysets).unwrap();
            c.count();
            c.commit(store).await.unwrap();
            let records = store.get_counters(&mint_url, &pubkey).await.unwrap();
            assert_eq!(vec![c.record().clone()], records);

            c.count();
            c.commit(store).await.unwrap();
            let records = store.get_counters(&mint_url, &pubkey).await.unwrap();
            assert_eq!(vec![c.record().clone()], records);

            let mut manager = Manager::new(&mint_url)
                .mnemonic(c.mnemonic().cloned())
                .records(records, &keysets);
            let mut c = manager.start_count(None, &keysets).unwrap();
            c.count();
            c.count();
            c.count();
            c.commit(store).await.unwrap();
            let records = store.get_counters(&mint_url, &pubkey).await.unwrap();
            assert_eq!(vec![c.record().clone()], records);
            let mut bks = records.clone();

            std::mem::drop(c);
            let mut c = manager.start_count(Some("usd"), &keysets).unwrap();
            assert_eq!(c.count(), 0);
            assert_eq!(c.count(), 1);
            c.commit(store).await.unwrap();
            assert_eq!(c.record().counter, 2);
            assert_eq!(c.count(), 2);
            c.commit(store).await.unwrap();
            assert_eq!(c.record().counter, 3);

            let records = store.get_counters(&mint_url, &pubkey).await.unwrap();

            assert_eq!(records.len(), 2);
            #[rustfmt::skip]
            assert_eq!(c.record(), records.iter().find(|r|r.keysetid==c.record().keysetid).unwrap());

            bks.push(c.record().clone());
            bks.sort_by(|a, b| cmp_by_asc(a.ts, b.ts));
            assert_eq!(bks, records);
            // for timestamp update
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        {
            let keysets = keysets.into_iter().skip(1).collect::<Vec<_>>();
            let records = store.get_counters(&mint_url, &pubkey).await.unwrap();
            assert_eq!(2, records.len());
            let mut bks = records.clone();

            let mut manager = Manager::new(&mint_url)
                .mnemonic(mnemonic.clone())
                .records(records, &keysets);
            let mut c = manager.start_count(None, &keysets).unwrap();
            assert_eq!(c.count(), 0);
            c.commit(store).await.unwrap();
            assert_eq!(c.before(), 1);
            assert_eq!(c.count(), 1);
            c.commit(store).await.unwrap();
            assert_eq!(c.before(), 2);
            let records = store.get_counters(&mint_url, &pubkey).await.unwrap();
            assert_eq!(records.len(), 3);

            bks.push(c.record().clone());
            bks.sort_by(|a, b| cmp_by_asc(a.ts, b.ts));
            assert_eq!(bks, records);
        }

        {
            let keysets = keysets_bk.clone();
            let records = store.get_counters(&mint_url, &pubkey).await.unwrap();
            assert_eq!(3, records.len());

            let mut manager = Manager::new(&mint_url)
                .mnemonic(mnemonic)
                .records(records, &keysets);
            let mut c = manager.start_count(None, &keysets).unwrap();
            assert_eq!(c.count(), 5);
            c.cancel();
            assert_eq!(c.count(), 5);
        }

        Ok(())
    }

    const KEYS_ID: &str = "00759e3f8b06b36f";
    const KEYS: &str = r#"{"1":"038a935c51c76c780ff9731cfbe9ab477f38346775809fa4c514340feabbec4b3a","2":"038288b12ebf2db3645e5d58835bd100398b6b19dfef338c698b55c05d0d41fb0a","4":"02fc8201cf4ea29abac0495d1304064f0e698762b8c0db145c1737b38a9d61c7e2","8":"02274243e03ca19f969acc7072812405b38adc672d1d753e65c63746b3f31cc6eb","16":"025f07cb2493351e7d5202f05eaf3934d5c9d17e73385e9de5bfab802f7d8caf92","32":"03afce0a897c858d7c88c1454d492eac43011e3396dda5b778ba1fcab381c748b1","64":"037b2178f42507f0c95e09d9b435a127df4b3e23ccd20af8075817d3abe90947ad","128":"02ebce8457b48407d4d248dba5a31b3eabf08a6285d09d08e40681c4adaf77bd40","256":"03c89713d27d6f8e328597b43dd87623efdcb251a484932f9e095ebfb6dbf4bdf2","512":"02df10f3ebba69916d03ab1754488770498f2e5466224d6df6d12811a13e46776c","1024":"02f5d9cba0502c21c6b39938a09dcb0390f124a2fd65e45dfeccd153cc1864273d","2048":"039de1dad91761b194e7674fb6ba212241aaf7f49dcb578a8fe093196ad1b20d1c","4096":"03cc694ba22e455f1c22b2cee4a40ecdd4f3bb4da0745411adb456158372d3efbb","8192":"029d66c24450fc315e046010df6870d61daa90c5c486c5ec4d7d3b99c5c2bce923","16384":"0387d063821010c7bd5cf79441870182f70cd432d13d3fc255e7b6ffd82c9d3c5a","32768":"021a94c6c03f7de8feb25b8a8b8d1f1c6f56af4bc533eb97c9e8b89c76b616ff11","65536":"038989c6ed91a7c577953115b465ee400a270a64e95eda8f7ee9d6bf30b8fe4908","131072":"03c3d3cd2523f004ee479a170b0ec5c74c060edb8356fc1b0a9ed8087cf6345172","262144":"02e54a7546f1a9194f30baa593a13d4e2949eb866593445d89675d7d394ef6320b","524288":"034e91037b3f1d3258d1e871dede80e98ef83e307c2e5ff589f38bd046f97546f8","1048576":"03306d42752a1adcfa394af2a690961ac9b80b1ac0f5fdc0890f66f8dc7d25ac6e","2097152":"03ec114332fe798c3e36675566c4748fda7d881000a01864ec48486512d7901e76","4194304":"02095e3e443d98ca3dfabcebc2f9154f3656b889783f7edb8290cfb01f497e63cf","8388608":"03c90f31525a4f9ab6562ec3edbf2bafc6662256ea6ce82ab19a45d2aee80b2f15","16777216":"03c0ae897a45724465c713c1379671ac5ff0a81c32e5f2dd27ea7e5530c7af484c","33554432":"034bcf793b70ba511e9c84cd07fc0c73c061e912bc02df4cac7871d048bad653b6","67108864":"021c6826c23a181d14962f43121943569a54f9d5af556eb839aee42d3f62debee6","134217728":"030e1bc651b6496922978d6cd3ed923cbf12b4332c496f841f506f5abf9d186d35","268435456":"03e3219e50cf389a75794f82ab4f880f5ffe9ca227b992c3e93cb4bb659d8e3353","536870912":"03879ad42536c410511ac6956b9da2d0da59ce7fbb6068bd9b25dd7cccddcc8096","1073741824":"03c4d3755a17904c0cfa7d7a21cc5b4e85fca8ac85369fcb12a6e2177525117dee","2147483648":"02e7a5d5cd3ea24f05f741dddad3dc8c5e24db60eb9bf9ad888b1c5dfbd792665e","4294967296":"03c783d24d8c9e51207eb3d6199bf48d6eb81a4b34103b422724be15501ff921bd","8589934592":"03200234495725455f4c4e6b6cb7b7936eb7cd1d1c9bb73d2ce032bae7d728b3ca","17179869184":"02eafa50ac67de2c206d1a67245b72ec20fac081c2a550294cc0a711246ed65a41","34359738368":"024c153c2a56de05860006aff9dc35ec9cafd7ac68708442a3a326c858b0c1a146","68719476736":"035a890c2d5c8bf259b98ac67d0d813b87778bcb0c0ea1ee9717ac804b0be3f563","137438953472":"025184ca832f08b105fdb471e2caf14025a1daa6f44ce90b4c7703878ccb6b26e8","274877906944":"039d19a41abdd49949c60672430018c63f27c5a28991f9fbb760499daccc63146c","549755813888":"03a138ac626dd3e6753459903aa128a13c052ed0058f2ead707c203bd4a7565237","1099511627776":"0298c8ef2eab728613103481167102efaf2d4b7a303cb94b9393da37a034a95c53","2199023255552":"02d88f8fc93cd2edf303fdebfecb70e59b5373cb8f746a1d075a9c86bc9382ac07","4398046511104":"02afd89ee23eee7d5fe6687fee898f64e9b01913ec71b5c596762b215e040c701f","8796093022208":"02196b461f3c804259e597c50e514920427aab4beaef0c666185fb2ff4399813db","17592186044416":"037b33746a6fd7a71d4cf17c85d13a64b98620614c0028d4995163f1b8484ee337","35184372088832":"036cce0a1878bbc63b3108c379ef4e6529fbf20ed675d80d91ca3ccc55fde4bdbd","70368744177664":"039c81dccb319ba70597cdf9db33b459164a1515c27366c8f667b01d988874e554","140737488355328":"036b2dd85a3c44c4458f0b246ce19a1524a191f1716834cfb452c6e1f946172c19","281474976710656":"022c84722c31a2b3d8cfd9b6a9e6199515fd97d6a9c390fc3d82f123bfc501ad04","562949953421312":"0355e2be85ee599b8fa7e6e68a9954573d032e89aa9e65c2e1231991664c200bf3","1125899906842624":"024b10818cd27f3eec6c9daf82b9dfa53928ab0711b711070bd39892ac10dee765","2251799813685248":"02a6d726432bb18c3145eba4fc0b587bf64f3be8617c0070dda33944474b3f8740","4503599627370496":"0248304be3cbaf31ec320bc636bb936c5984caf773df950fc44c6237ec09c557a1","9007199254740992":"03a3c0e9da7ece7d7b132c53662c0389bd87db801dff5ac9edd9f46699cb1dc065","18014398509481984":"03b6c4c874e2392072e17fbfd181afbd40d6766a8ca4cf932264ba98d98de1328c","36028797018963968":"0370dca4416ec6e30ff02f8e9db7804348b42e3f5c22099dfc896fa1b2ccbe7a69","72057594037927936":"0226250140aedb79de91cb4cc7350884bde229063f34ee0849081bb391a37c273e","144115188075855872":"02baef3a94d241aee9d6057c7a7ee7424f8a0bcb910daf6c49ddcabf70ffbc77d8","288230376151711744":"030f95a12369f1867ce0dbf2a6322c27d70c61b743064d76cfc81dd43f1a052ae6","576460752303423488":"021bc89118ab6eb1fbebe0fa6cc76da8236a7991163475a73a22d8efd016a45800","1152921504606846976":"03b0c1e658d7ca12830a0b590ea5a4d6db51084ae80b6d8abf27ad2d762209acd1","2305843009213693952":"0266926ce658a0bdae934071f22e09dbb6ecaff2a4dc4b1f8e23626570d993b48e","4611686018427387904":"03ac17f10f9bb745ebd8ee9cdca1b6981f5a356147d431196c21c6d4869402bde0","9223372036854775808":"037ab5b88c8ce34c4a3970be5c6f75b8a7a5493a12ef56a1c9ba9ff5f90de46fcc"}"#;
    fn random_proofs(amounts: &[u64]) -> ProofsExtended {
        let keyset: Map<u64, String> = serde_json::from_str(KEYS).unwrap();

        let mut ps = vec![];

        for a in amounts {
            let a2s = cashu::Amount::from(*a).split();
            let coins = a2s.iter().map(|a| a.to_u64()).collect::<Vec<_>>();
            println!("{}: {:?}", a, coins);

            for a2 in a2s {
                // let r = rand::random::<[u8; 32]>();

                let p = cashu::nuts::Proof {
                    amount: a2,
                    secret: cashu::secret::Secret::generate(),
                    c: keyset.get(&a2.to_u64()).unwrap().parse().unwrap(),
                    keyset_id: KEYS_ID.parse().unwrap(),
                    witness: None,
                    dleq: None,
                };

                ps.push(p.into());
            }
        }

        ps
    }

    pub async fn test_proof<S: UnitedStore>(
        store: &S,
        ts_fill_all: Option<bool>,
    ) -> Result<(), S::Error> {
        let a = rand::random::<u64>() % 10000_00 + 1;
        let mut proofs = random_proofs([a].as_slice());
        assert_eq!(a, proofs.sum().to_u64());

        // test basic compat for old version
        for p in &mut proofs {
            assert!(p.unit.is_none());
            if rand::random() {
                p.unit = Some(CURRENCY_UNIT_SAT.to_owned());
            }

            assert!(p.ts.is_none());
            if ts_fill_all.unwrap_or_default() || rand::random() {
                p.ts = Some(unixtime_ms());
            }
        }

        let mint_url = MINT_URL.parse::<Url>().unwrap();
        store.add_proofs(&mint_url, &proofs).await.unwrap();
        let mut ps = store
            .get_proofs_limit_unit(&mint_url, CURRENCY_UNIT_SAT)
            .await
            .unwrap();
        assert_eq!(a, ps.sum().to_u64());
        let mut psa = store.get_all_proofs().await.unwrap();
        assert_eq!(psa.len(), 1);

        proofs.sort_by(|a, b| a.as_ref().amount.cmp(&b.as_ref().amount));
        ps.sort_by(|a, b| a.as_ref().amount.cmp(&b.as_ref().amount));
        assert_eq!(proofs, ps);

        let mint_unit = MintUrlWithUnit::new(MINT_URL, CURRENCY_UNIT_SAT).into_owned();
        psa.get_mut(&mint_unit)
            .unwrap()
            .sort_by(|a, b| a.as_ref().amount.cmp(&b.as_ref().amount));
        assert_eq!(proofs, *psa.get(&mint_unit).unwrap());

        let split = cashu::Amount::from(ps.len() as u64)
            .split()
            .into_iter()
            .map(|a| a.to_u64() as usize)
            .collect::<std::collections::HashSet<_>>();
        for s in split {
            let mut delete = vec![];
            for _ in 0..s {
                let d = rand::random::<usize>() % ps.len();
                let p = ps.remove(d);
                delete.push(p);
            }

            store.delete_proofs(&mint_url, &delete).await?;
            let mut ps2 = store
                .get_proofs_limit_unit(&mint_url, CURRENCY_UNIT_SAT)
                .await
                .unwrap();
            ps2.sort_by(|a, b| a.as_ref().amount.cmp(&b.as_ref().amount));
            assert_eq!(ps2.sum(), ps.sum());
            assert_eq!(ps2, ps);
        }

        let ps2 = store
            .get_proofs_limit_unit(&mint_url, CURRENCY_UNIT_SAT)
            .await
            .unwrap();
        assert_eq!(ps2, vec![]);

        Ok(())
    }

    use crate::wallet::{MintProofsGeneric, TokenV3Generic};
    fn random_tokens(amounts: &[u64]) -> TokenV3Generic<ProofsExtended> {
        let mut tokens = TokenV3Generic {
            token: vec![],
            memo: None,
            unit: None,
        };
        for a in amounts {
            let proofs = random_proofs(&[*a]);
            let p = MintProofsGeneric {
                mint: MINT_URL.parse().unwrap(),
                proofs,
            };

            tokens.token.push(p);
        }

        tokens
    }

    pub async fn test_transaction_cashu<S: UnitedStore + Sync>(store: &S) -> Result<(), S::Error> {
        let tokens = random_tokens(&[1]);
        let token = tokens.to_string();

        let mut tx0 = CashuTransaction {
            id: crate::types::hashid(&token),
            status: TransactionStatus::Pending,
            io: TransactionDirection::Out,
            info: None,
            time: unixtime_ms(),
            amount: 1,
            mint: MINT_URL.to_string(),
            unit: None,
            token,
        };

        let tx = tx0.clone().into();
        store.add_transaction(&tx).await?;
        let txgot = store.get_transaction(tx.id()).await?;
        assert_eq!(*txgot.as_ref().unwrap(), tx);
        let txs = store.get_all_transactions().await?;
        assert_eq!(txs, vec![txgot.unwrap()]);
        let txs_pending = store.get_transactions(&[tx0.status]).await?;
        assert_eq!(txs_pending, txs);
        #[rustfmt::skip]
        let txs_pending = store.get_transactions(&[TransactionStatus::Success]).await?;
        assert_eq!(txs_pending, vec![]);

        tx0.status = TransactionStatus::Success;
        let tx = tx0.clone().into();
        store.add_transaction(&tx).await?;
        let txgot = store.get_transaction(tx.id()).await?;
        assert_eq!(*txgot.as_ref().unwrap(), tx);
        let txs = store.get_all_transactions().await?;
        assert_eq!(txs, vec![txgot.unwrap()]);
        let txs_pending = store.get_transactions(&[tx0.status]).await?;
        assert_eq!(txs_pending, txs);
        #[rustfmt::skip]
        let txs_pending = store.get_transactions(&[TransactionStatus::Pending]).await?;
        assert_eq!(txs_pending, vec![]);

        #[rustfmt::skip]
        let dc = store.delete_transactions(&[tx0.status], tx0.time-1).await?;
        assert_eq!(dc, 0);
        let dc = store.delete_transactions(&[tx0.status], tx0.time).await?;
        assert_eq!(dc, 1);
        let txgot = store.get_transaction(tx.id()).await?;
        assert_eq!(txgot, None);
        let txs = store.get_all_transactions().await?;
        assert_eq!(txs, vec![]);
        #[rustfmt::skip]
        let txs_pending = store.get_transactions(&[TransactionStatus::Pending]).await?;
        assert_eq!(txs_pending, vec![]);
        #[rustfmt::skip]
        let txs_pending = store.get_transactions(&[TransactionStatus::Success]).await?;
        assert_eq!(txs_pending, vec![]);
        #[rustfmt::skip]
        let txs_pending = store.get_transactions(&[TransactionStatus::Failed]).await?;
        assert_eq!(txs_pending, vec![]);

        Ok(())
    }

    pub async fn test_transaction_ln<S: UnitedStore + Sync>(store: &S) -> Result<(), S::Error> {
        let pr = "lnbc1m1pjslwjhsp5zyntvam8ys92t4m2qxmmva0dulqnr6l4mscnwwwdzawlq9cevx4qpp57vfpu3jffd0tyvg8fj93vggvwxqud8stvdwzer0fpha8ru5rpqnqdq4gdshx6r4ypjx2ur0wd5hgxqzjccqpjrzjqg7dvuzvu7ryfftgl0ve8ajacahmr0utenjvjy5nq3ruw8gvy6v26rq9e5qqwvqqquqqqqqqqqqqqxgq9q9qxpqysgqg4gj9vsd80ff0zcl25hsh2akg54dfhy2dez9ztgl9zvznt4lf2k860juys8tpenkaq933tf9ssns52lmcqmar6a9rjdg2nmfwxz8edgptd732x";
        let hash = "Ewh2Og86r9jsLXbgLJrdWoqgO3mjXSKV-HAYSpDz";
        let token: cashu::Bolt11Invoice = pr.parse().unwrap();

        let mut tx0 = LNTransaction {
            status: TransactionStatus::Pending,
            io: TransactionDirection::Out,
            info: None,
            time: unixtime_ms(),
            amount: 1,
            mint: MINT_URL.to_string(),
            pr: token.to_string(),
            hash: hash.to_owned(),
            fee: None,
            unit: None,
        };

        println!("hash: {}, hashg: {}", hash, tx0.id(),);

        let tx = tx0.clone().into();
        store.add_transaction(&tx).await?;
        let txgot = store.get_transaction(tx.id()).await?;
        assert_eq!(*txgot.as_ref().unwrap(), tx);
        let txs = store.get_all_transactions().await?;
        assert_eq!(txs, vec![txgot.unwrap()]);
        let txs_pending = store.get_transactions(&[tx0.status]).await?;
        assert_eq!(txs_pending, txs);
        #[rustfmt::skip]
        let txs_pending = store.get_transactions(&[TransactionStatus::Success]).await?;
        assert_eq!(txs_pending, vec![]);

        tx0.status = TransactionStatus::Success;
        let tx = tx0.clone().into();
        store.add_transaction(&tx).await?;
        let txgot = store.get_transaction(tx.id()).await?;
        assert_eq!(*txgot.as_ref().unwrap(), tx);
        let txs = store.get_all_transactions().await?;
        assert_eq!(txs, vec![txgot.unwrap()]);
        let txs_pending = store.get_transactions(&[tx0.status]).await?;
        assert_eq!(txs_pending, txs);
        #[rustfmt::skip]
        let txs_pending = store.get_transactions(&[TransactionStatus::Pending]).await?;
        assert_eq!(txs_pending, vec![]);

        #[rustfmt::skip]
        let dc = store.delete_transactions(&[tx0.status], tx0.time-1).await?;
        assert_eq!(dc, 0);
        let dc = store.delete_transactions(&[tx0.status], tx0.time).await?;
        assert_eq!(dc, 1);
        let txgot = store.get_transaction(tx.id()).await?;
        assert_eq!(txgot, None);
        let txs = store.get_all_transactions().await?;
        assert_eq!(txs, vec![]);
        #[rustfmt::skip]
        let txs_pending = store.get_transactions(&[TransactionStatus::Pending]).await?;
        assert_eq!(txs_pending, vec![]);
        #[rustfmt::skip]
        let txs_pending = store.get_transactions(&[TransactionStatus::Success]).await?;
        assert_eq!(txs_pending, vec![]);
        #[rustfmt::skip]
        let txs_pending = store.get_transactions(&[TransactionStatus::Failed]).await?;
        assert_eq!(txs_pending, vec![]);

        Ok(())
    }
}
