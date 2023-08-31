use strum::{AsRefStr, Display, EnumIs, EnumString, IntoStaticStr};

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
//
#[derive(Display, AsRefStr, IntoStaticStr, EnumIs, EnumString)]
pub enum TransactionStatus {
    Pending,
    Success,
    Failed,
    Expired,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
//
#[derive(Display, AsRefStr, IntoStaticStr, EnumIs, EnumString)]
pub enum TransactionDirection {
    In,
    Out,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
//
#[derive(Display, AsRefStr, IntoStaticStr, EnumIs, EnumString)]
pub enum TransactionKind {
    Cashu,
    LN,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
//
#[derive(EnumIs)]
#[serde(tag = "kind")]
pub enum Transaction {
    Cashu(CashuTransaction),
    LN(LNTransaction),
}

impl Transaction {
    pub fn time(&self) -> u64 {
        match self {
            Transaction::Cashu(transaction) => transaction.time,
            Transaction::LN(transaction) => transaction.time,
        }
    }

    pub fn amount(&self) -> u64 {
        match self {
            Transaction::Cashu(transaction) => transaction.amount,
            Transaction::LN(transaction) => transaction.amount,
        }
    }

    pub fn direction(&self) -> TransactionDirection {
        match self {
            Transaction::Cashu(transaction) => transaction.io,
            Transaction::LN(transaction) => transaction.io,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Transaction::Cashu(transaction) => transaction.id(),
            Transaction::LN(transaction) => transaction.id(),
        }
    }

    pub fn status(&self) -> TransactionStatus {
        match self {
            Transaction::Cashu(transaction) => transaction.status,
            Transaction::LN(transaction) => transaction.status,
        }
    }

    pub fn status_mut(&mut self) -> &mut TransactionStatus {
        match self {
            Transaction::Cashu(ref mut transaction) => &mut transaction.status,
            Transaction::LN(ref mut transaction) => &mut transaction.status,
        }
    }

    pub fn info(&self) -> Option<&str> {
        match self {
            Transaction::Cashu(transaction) => transaction.info.as_deref(),
            Transaction::LN(transaction) => transaction.info.as_deref(),
        }
    }

    pub fn info_mut(&mut self) -> &mut Option<String> {
        match self {
            Transaction::Cashu(transaction) => &mut transaction.info,
            Transaction::LN(transaction) => &mut transaction.info,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.status() == TransactionStatus::Pending
    }

    pub fn content(&self) -> &str {
        match self {
            Transaction::Cashu(transaction) => &transaction.token,
            Transaction::LN(transaction) => &transaction.pr,
        }
    }

    pub fn mint_url(&self) -> &str {
        match self {
            Transaction::Cashu(transaction) => transaction.mint.as_str(),
            Transaction::LN(transaction) => transaction.mint.as_str(),
        }
    }

    pub fn fee(&self) -> Option<u64> {
        match self {
            Transaction::Cashu(_) => None,
            Transaction::LN(transaction) => transaction.fee,
        }
    }

    pub fn as_json(&self) -> String {
        serde_json::to_string(self).expect("json encode")
    }

    pub fn kind(&self) -> TransactionKind {
        match self {
            Transaction::Cashu(_transaction) => TransactionKind::Cashu,
            Transaction::LN(_transaction) => TransactionKind::LN,
        }
    }

    pub fn unit(&self) -> Option<&str> {
        match self {
            Transaction::Cashu(_transaction) => _transaction.unit.as_deref(),
            Transaction::LN(_transaction) => _transaction.unit.as_deref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashuTransaction {
    pub id: String,
    pub status: TransactionStatus,
    pub io: TransactionDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
    pub time: u64,
    pub amount: u64,
    pub mint: String,
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

pub fn hashid(data: impl AsRef<[u8]>) -> String {
    hex::encode(sha256::Hash::hash(data.as_ref()))
}

use bitcoin_hashes::sha256;
use bitcoin_hashes::Hash;
impl CashuTransaction {
    pub fn new(
        status: TransactionStatus,
        io: TransactionDirection,
        amount: u64,
        mint: &str,
        token: &str,
        time: Option<u64>,
        unit: Option<&str>,
    ) -> Self {
        let this = Self {
            id: hashid(token),
            status,
            io,
            amount,
            info: None,
            time: time.unwrap_or_else(unixtime_ms),
            mint: mint.to_string(),
            token: token.to_string(),
            unit: unit.map(|s| s.to_owned()),
        };

        this
    }
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl From<CashuTransaction> for Transaction {
    fn from(val: CashuTransaction) -> Self {
        Transaction::Cashu(val)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LNTransaction {
    pub status: TransactionStatus,
    pub io: TransactionDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
    pub time: u64,
    pub amount: u64,
    pub fee: Option<u64>,
    pub mint: String,
    // invoice
    // default expire time: 600s
    pub pr: String,
    pub hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}
/*
{"pr":"lnbc1m1pjslwjhsp5zyntvam8ys92t4m2qxmmva0dulqnr6l4mscnwwwdzawlq9cevx4qpp57vfpu3jffd0tyvg8fj93vggvwxqud8stvdwzer0fpha8ru5rpqnqdq4gdshx6r4ypjx2ur0wd5hgxqzjccqpjrzjqg7dvuzvu7ryfftgl0ve8ajacahmr0utenjvjy5nq3ruw8gvy6v26rq9e5qqwvqqquqqqqqqqqqqqxgq9q9qxpqysgqg4gj9vsd80ff0zcl25hsh2akg54dfhy2dez9ztgl9zvznt4lf2k860juys8tpenkaq933tf9ssns52lmcqmar6a9rjdg2nmfwxz8edgptd732x",
"hash":"Ewh2Og86r9jsLXbgLJrdWoqgO3mjXSKV-HAYSpDz"}

https://github.com/cashubtc/nuts/blob/main/03.md

with pr being the bolt11 payment request and hash is a random hash generated by the mint to internally look up the invoice state.
Note: hash MUST NOT be the bolt11 invoice hash as it needs to be a secret between mint and user.
This is to prevent a third party who knows the bolt11 invoice from stealing the tokens when the invoice is paid (See NUT-04).
hash MUST be url-safe since we use it in a URL as described in NUT-04.

A wallet MUST store the hash and amount_sat in its database to later request the tokens upon paying the invoice.
A wallet SHOULD then present the payment request (for example via QR code) to the user such that they can pay the invoice with another Lightning wallet.
After the user has paid the invoice, a wallet MUST continue with NUT-04 (minting tokens).
*/
impl LNTransaction {
    pub fn new(
        status: TransactionStatus,
        io: TransactionDirection,
        amount: u64,
        fee: Option<u64>,
        mint: &str,
        pr: &str,
        hash: &str,
        time: Option<u64>,
        unit: Option<&str>,
    ) -> Self {
        let this = Self {
            status,
            io,
            amount,
            fee,
            mint: mint.to_string(),
            info: None,
            time: time.unwrap_or_else(unixtime_ms),
            pr: pr.to_string(),
            hash: hash.to_string(),
            unit: unit.map(|s| s.to_owned()),
        };
        this
    }
    pub fn id(&self) -> &str {
        &self.hash
    }
}
impl From<LNTransaction> for Transaction {
    fn from(val: LNTransaction) -> Self {
        Transaction::LN(val)
    }
}

pub fn unixtime_ms() -> u64 {
    use std::time::SystemTime;

    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|x| x.as_millis() as u64)
        .unwrap_or(0)
}

// redefine for flutter frb
// https://github.com/cashubtc/nuts/blob/main/06.md
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MintInfo {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pubkey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_long: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motd: Option<String>,
    #[serde(default)]
    pub contact: Vec<Vec<String>>,
    pub nuts: Nuts,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Nuts {
    #[serde(default, rename = "4")]
    pub nut04: PaymentMethodSettings,
    #[serde(default, rename = "5")]
    pub nut05: PaymentMethodSettings,
    #[serde(default, rename = "7")]
    pub nut07: NutSupported,
    #[serde(default, rename = "8")]
    pub nut08: NutSupported,
    #[serde(default, rename = "9")]
    pub nut09: NutSupported,
    #[serde(default, rename = "10")]
    pub nut10: NutSupported,
    #[serde(default, rename = "11")]
    pub nut11: NutSupported,
    #[serde(default, rename = "12")]
    pub nut12: NutSupported,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaymentMethod {
    pub method: String,
    pub unit: String,
    #[serde(default)]
    pub min_amount: i64,
    #[serde(default)]
    pub max_amount: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaymentMethodSettings {
    #[serde(default)]
    pub methods: Vec<PaymentMethod>,
    // compat for old json in database
    // #[serde(default)]
    pub disabled: bool,
}

// default should disabled
impl Default for PaymentMethodSettings {
    fn default() -> Self {
        Self {
            methods: vec![],
            disabled: true,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NutSupported {
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mint {
    pub url: String,
    pub active: bool,
    pub time: u64,
    pub info: Option<MintInfo>,
}

impl Mint {
    pub fn new(url: String, info: Option<MintInfo>) -> Self {
        Self {
            url,
            info,
            active: true,
            time: unixtime_ms(),
        }
    }
}

// #[cfg(test)]
pub mod tests {
    #[allow(unused_imports)]
    use super::*;

    pub const MINT_URL: &str = "https://8333.space:3338/";
    pub const MINT_URL_TEST: &str = "https://testnut.cashu.space/";

    // Nutshell/0.15.3
    // curl -X GET https://8333.space:3338/v1/info
    pub const INFO: &str = r#"{"name":"Cashu test mint","pubkey":"03e3d23e1b66eadaf15ce0d640a908e8ba1984baed34ab98c547aab4cf4249440d","version":"Nutshell/0.15.3","description":"This mint is for testing and development purposes only. Do not use this mint as a default mint in your application! Please use it with caution and only with very small amounts. Your Cashu client could have bugs. Accidents and bugs can lead to loss of funds for which we are not responsible for.","contact":[["",""]],"nuts":{"4":{"methods":[{"method":"bolt11","unit":"sat","min_amount":0,"max_amount":100000}],"disabled":false},"5":{"methods":[{"method":"bolt11","unit":"sat","min_amount":0,"max_amount":50000}],"disabled":false},"7":{"supported":true},"8":{"supported":true},"9":{"supported":true},"10":{"supported":true},"11":{"supported":true},"12":{"supported":true}}}"#;
    // curl https://testnut.cashu.space/v1/info
    // min_amount is null
    pub const INFO_TEST: &str = r#"{"name":"Cashu mint","pubkey":"0296d0aa13b6a31cf0cd974249f28c7b7176d7274712c95a41c7d8066d3f29d679","version":"Nutshell/0.15.3","contact":[["",""]],"nuts":{"4":{"methods":[{"method":"bolt11","unit":"sat"},{"method":"bolt11","unit":"usd"}],"disabled":false},"5":{"methods":[{"method":"bolt11","unit":"sat"},{"method":"bolt11","unit":"usd"}],"disabled":false},"7":{"supported":true},"8":{"supported":true},"9":{"supported":true},"10":{"supported":true},"11":{"supported":true},"12":{"supported":true}}}"#;
    // curl https://mint.minibits.cash/Bitcoin/v1/info

    // curl https://bitcointxoko.com/cashu/api/v1/dMk78c5aR7uhHzcqH3Bwqp/v1/info
    // LNbitsCashu/0.4.5
    // publickey null
    pub const INFO_LNBITS: &str = r#"{"name":"STPI Cashu Mint","version":"LNbitsCashu/0.5","description":"STPI mint","description_long":"","nuts":{"4":{"methods":[["bolt11","sat"]],"disabled":true},"5":{"methods":[["bolt11","sat"]],"disabled":false},"7":{"supported":true},"8":{"supported":true},"9":{"supported":true},"10":{"supported":true},"11":{"supported":true},"12":{"supported":true}}}"#;

    #[test]
    fn test_06_mint_information_pro() {
        let js: MintInfo = serde_json::from_str(INFO).unwrap();
        assert_eq!(js.name, "Cashu test mint");
        assert_eq!(js.nuts.nut04.disabled, false);
        assert!(js.nuts.nut04.methods.len() > 0);
        assert_eq!(js.nuts.nut05.disabled, false);
        assert!(js.nuts.nut05.methods.len() > 0);
    }

    #[test]
    fn test_06_mint_information_test() {
        let js: MintInfo = serde_json::from_str(INFO_TEST).unwrap();
        assert_eq!(js.name, "Cashu mint");
        assert_eq!(js.nuts.nut04.disabled, false);
        assert!(js.nuts.nut04.methods.len() > 0);
        assert_eq!(js.nuts.nut05.disabled, false);
        assert!(js.nuts.nut05.methods.len() > 0);
    }

    #[test]
    fn test_06_mint_information_lnbits() {
        let js: MintInfo = serde_json::from_str(INFO_LNBITS).unwrap();
        assert_eq!(js.name, "STPI Cashu Mint");
        assert_eq!(js.nuts.nut04.disabled, true);
        assert!(js.nuts.nut04.methods.len() > 0);
        assert_eq!(js.nuts.nut05.disabled, false);
        assert!(js.nuts.nut05.methods.len() > 0);
    }
}
