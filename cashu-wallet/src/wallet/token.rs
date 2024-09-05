use base64::{alphabet, engine::general_purpose, Engine};
use cashu::{
    nuts::nut00::Error,
    nuts::{CurrencyUnit, Proof, Proofs},
    Amount,
};
use serde::Deserialize;
use url::Url;

pub type ProofsExtended = Vec<ProofExtended>;

// for store to kvdb with unit
#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct ProofExtended {
    #[serde(flatten)]
    pub raw: Proof,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    // backup json string for kvdb delete: the order of feilds changed(v1pre vs v1)
    #[serde(skip)]
    pub js: String,
}

impl ProofExtended {
    pub fn unit(&self) -> Option<&str> {
        self.unit.as_deref()
    }
    pub fn json(mut self, js: String) -> Self {
        self.js = js;
        self
    }
}

// skip js for test kvdb
impl PartialEq for ProofExtended {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw && self.ts == other.ts && self.unit == other.unit
    }
}

impl AsRef<Proof> for ProofExtended {
    fn as_ref(&self) -> &Proof {
        &self.raw
    }
}

impl From<Proof> for ProofExtended {
    fn from(raw: Proof) -> Self {
        Self {
            raw,
            ts: None,
            unit: None,
            js: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofsSerdeToRaw<'a, T: AsRef<Proof>> {
    pub(crate) raw: &'a [T],
}

impl<'a, T: AsRef<Proof>> serde::Serialize for ProofsSerdeToRaw<'a, T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut state = serializer.serialize_seq(Some(self.raw.len()))?;
        for element in self.raw {
            let p: &Proof = element.as_ref();
            state.serialize_element(p)?;
        }
        state.end()
    }
}

impl<'a> From<&'a ProofsExtended> for ProofsSerdeToRaw<'a, ProofExtended> {
    fn from(raw: &'a ProofsExtended) -> Self {
        Self { raw }
    }
}

use crate::types::unixtime_ms;

/// helper for Proofs
pub trait ProofsHelper: Sized + serde::Serialize {
    type Proof: AsRef<Proof>;
    fn as_slice(&self) -> &[Self::Proof];
    fn sum(&self) -> Amount {
        self.as_slice().iter().map(|p| p.as_ref().amount).sum()
    }
    fn to_serde_raw(&self) -> ProofsSerdeToRaw<'_, Self::Proof> {
        ProofsSerdeToRaw {
            raw: self.as_slice(),
        }
    }
    fn to_extended(&self) -> ProofsExtended;
    fn into_extended(self) -> ProofsExtended {
        self.to_extended()
    }
    fn to_extended_with_unit(&self, unit: Option<&str>) -> ProofsExtended {
        self.to_extended().into_extended_with_unit(unit)
    }
    fn into_extended_with_unit(self, unit: Option<&str>) -> ProofsExtended {
        let mut ps = self.into_extended();
        for p in &mut ps {
            p.ts = Some(unixtime_ms());
            p.unit = unit.map(|s| s.to_owned());
        }
        ps
    }
}

impl ProofsHelper for &[ProofExtended] {
    type Proof = ProofExtended;
    fn as_slice(&self) -> &[Self::Proof] {
        self
    }
    fn to_extended(&self) -> ProofsExtended {
        self.to_vec()
    }
}

impl ProofsHelper for ProofsExtended {
    type Proof = ProofExtended;
    fn as_slice(&self) -> &[Self::Proof] {
        &self[..]
    }
    fn to_extended(&self) -> ProofsExtended {
        self.as_slice().to_extended()
    }
    fn into_extended(self) -> ProofsExtended {
        self
    }
}

impl ProofsHelper for &ProofsExtended {
    type Proof = ProofExtended;
    fn as_slice(&self) -> &[Self::Proof] {
        &self[..]
    }
    fn to_extended(&self) -> ProofsExtended {
        self.as_slice().to_extended()
    }
}

impl ProofsHelper for &[Proof] {
    type Proof = Proof;
    fn as_slice(&self) -> &[Self::Proof] {
        self
    }
    fn to_extended(&self) -> ProofsExtended {
        let mut ps = Vec::with_capacity(self.as_slice().len());
        for p in self.as_slice() {
            ps.push(p.as_ref().clone().into());
        }
        ps
    }
}

impl ProofsHelper for Proofs {
    type Proof = Proof;
    fn as_slice(&self) -> &[Self::Proof] {
        &self[..]
    }
    fn to_extended(&self) -> ProofsExtended {
        self.as_slice().to_extended()
    }
    fn into_extended(self) -> ProofsExtended {
        let mut ps = Vec::with_capacity(self.as_slice().len());
        for p in self {
            ps.push(p.into());
        }
        ps
    }
}

impl ProofsHelper for &Proofs {
    type Proof = Proof;
    fn as_slice(&self) -> &[Self::Proof] {
        &self[..]
    }
    fn to_extended(&self) -> ProofsExtended {
        self.as_slice().to_extended()
    }
}

pub type TokenV3 = TokenV3Generic<Proofs>;
// pub type TokenRef = TokenGeneric<[Proof]>;
pub type TokenV3Extened = TokenV3Generic<ProofsExtended>;
// pub type TokenExtenedRef = TokenGeneric<[ProofExtended]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenV3Generic<T: ProofsHelper> {
    pub token: Vec<MintProofsGeneric<T>>,
    /// Memo for token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    /// Token Unit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<CurrencyUnit>,
}

impl<T: ProofsHelper> TokenV3Generic<T> {
    pub fn new(
        mint_url: MintUrl,
        proofs: T,
        memo: Option<String>,
        unit: Option<CurrencyUnit>,
    ) -> Result<Self, Error> {
        if proofs.as_slice().is_empty() {
            return Err(Error::ProofsRequired);
        }

        Ok(Self {
            token: vec![MintProofsGeneric::new(mint_url, proofs)],
            memo,
            unit,
        })
    }

    pub fn amount(&self) -> u64 {
        use super::AmountHelper;

        self.token
            .iter()
            .map(|mps| mps.proofs.sum())
            .sum::<Amount>()
            .to_u64()
    }

    pub fn unit(&self) -> Option<&str> {
        self.unit.as_ref().map(|s| s.as_str())
    }

    pub fn mint0(&self) -> &Url {
        &self.token[0].mint.as_ref()
    }
}

impl std::str::FromStr for TokenV3 {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = if s.starts_with("cashuA") {
            s.replace("cashuA", "")
        } else {
            return Err(Error::UnsupportedToken);
        };

        let decode_config = general_purpose::GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent);
        let decoded =
            general_purpose::GeneralPurpose::new(&alphabet::STANDARD, decode_config).decode(s)?;
        let decoded_str = String::from_utf8(decoded)?;
        let token: Self = serde_json::from_str(&decoded_str)?;
        Ok(token)
    }
}

use std::fmt;
impl<T: ProofsHelper> fmt::Display for TokenV3Generic<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let json_string = serde_json::to_string(self).map_err(|_| fmt::Error)?;
        let encoded = general_purpose::STANDARD.encode(json_string);
        write!(f, "cashuA{}", encoded)
    }
}

pub type MintProofs = MintProofsGeneric<Proofs>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MintProofsGeneric<T: ProofsHelper> {
    pub mint: MintUrl,
    pub proofs: T,
}

impl<T: ProofsHelper> MintProofsGeneric<T> {
    fn new(mint_url: MintUrl, proofs: T) -> Self {
        Self {
            mint: mint_url,
            proofs,
        }
    }
}

use cashu::nuts::nut00::ProofV4;
impl Into<TokenV3> for TokenV4 {
    fn into(self) -> TokenV3 {
        let TokenV4 {
            mint_url,
            memo,
            unit,
            token,
        } = self;

        let mut proofs = Vec::with_capacity(token.iter().map(|t| t.proofs.len()).sum());
        for t in token {
            for p in t.proofs {
                let ProofV4 {
                    amount,
                    secret,
                    c,
                    witness,
                    dleq,
                } = p;

                let proof = Proof {
                    keyset_id: t.keyset_id,
                    amount,
                    secret,
                    c,
                    witness,
                    dleq,
                };

                proofs.push(proof);
            }
        }

        TokenV3::new(mint_url, proofs, memo, unit).unwrap()
    }
}

use cashu::nuts::nut00::token::TokenV4Token;
/// Token V4
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenV4 {
    /// Mint Url
    #[serde(rename = "m")]
    pub mint_url: MintUrl,
    /// Token Unit
    #[serde(rename = "u", skip_serializing_if = "Option::is_none")]
    pub unit: Option<CurrencyUnit>,
    /// Memo for token
    #[serde(rename = "d", skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    /// Proofs
    ///
    /// Proofs separated by keyset_id
    #[serde(rename = "t")]
    pub token: Vec<TokenV4Token>,
}

impl TokenV4 {
    pub fn new(
        mint_url: MintUrl,
        proofs: impl ProofsHelper,
        memo: Option<String>,
        unit: Option<CurrencyUnit>,
    ) -> Result<Self, Error> {
        let mut map = std::collections::BTreeMap::new();
        for p in proofs.as_slice() {
            let p = p.as_ref();
            let id = p.keyset_id;
            let proof = ProofV4 {
                c: p.c,
                amount: p.amount,
                secret: p.secret.clone(),
                witness: p.witness.clone(),
                dleq: p.dleq.clone(),
            };

            let entry = map.entry(id).or_insert_with(Vec::new);
            entry.push(proof);
        }

        let token = Self {
            mint_url,
            memo,
            unit,
            token: map
                .into_iter()
                .map(|(k, v)| TokenV4Token {
                    keyset_id: k,
                    proofs: v,
                })
                .collect(),
        };

        Ok(token)
    }
}

impl fmt::Display for TokenV4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use serde::ser::Error;
        let mut data = Vec::new();
        ciborium::into_writer(self, &mut data).map_err(|e| fmt::Error::custom(e.to_string()))?;

        let encode_config = general_purpose::GeneralPurposeConfig::new().with_encode_padding(false);
        let encoded = GeneralPurpose::new(&alphabet::URL_SAFE, encode_config).encode(data);
        write!(f, "cashuB{}", encoded)
    }
}

use base64::engine::GeneralPurpose;
impl std::str::FromStr for TokenV4 {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix("cashuB").ok_or(Error::UnsupportedToken)?;
        let decode_config = general_purpose::GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent);
        let decoded = GeneralPurpose::new(&alphabet::URL_SAFE, decode_config).decode(s)?;
        let token: TokenV4 = ciborium::from_reader(&decoded[..])?;

        // prevent empty token
        if token.token.iter().map(|t| t.proofs.len()).sum::<usize>() == 0 {
            return Err(Error::ProofsRequired);
        }
        Ok(token)
    }
}

use strum::EnumIs;
/// Token Enum
#[derive(EnumIs, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Token {
    /// Token V3
    TokenV3(TokenV3),
    /// Token V4
    TokenV4(TokenV4),
}

impl Into<Token> for TokenV3 {
    fn into(self) -> Token {
        Token::TokenV3(self)
    }
}
impl Into<Token> for TokenV4 {
    fn into(self) -> Token {
        Token::TokenV4(self)
    }
}

impl Token {
    pub fn into_v3(self) -> Result<TokenV3, Error> {
        let t = match self {
            Token::TokenV3(t) => t,
            Token::TokenV4(t) => t.into(),
        };
        Ok(t)
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let token = match self {
            Self::TokenV3(token) => token.to_string(),
            Self::TokenV4(token) => token.to_string(),
        };

        write!(f, "{}", token)
    }
}

impl std::str::FromStr for Token {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() < 7 {
            return Err(Error::UnsupportedToken);
        }
        let prefix6 = &s[..6];
        let token = match prefix6 {
            "cashuB" => {
                let t = TokenV4::from_str(s)?;
                Self::TokenV4(t)
            }
            "cashuA" => {
                let t = TokenV3::from_str(s)?;
                Self::TokenV3(t)
            }
            _ => return Err(Error::UnsupportedToken),
        };

        Ok(token)
    }
}

/// wrap Url for compat all
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MintUrl {
    raw: Url,
}

impl MintUrl {
    pub fn as_str(&self) -> &str {
        self.raw.as_str()
    }
}

// https://8333.space:3338 -> https://8333.space:3338/
// https://mint.minibits.cash/Bitcoin -> https://mint.minibits.cash/Bitcoin/
// not endswith / join not work
impl From<Url> for MintUrl {
    fn from(mut url: Url) -> Self {
        if !url.path().ends_with("/") {
            url.set_path(&format!("{}/", url.path()))
        }
        Self { raw: url }
    }
}
impl Into<Url> for MintUrl {
    fn into(self) -> Url {
        self.raw
    }
}

impl std::str::FromStr for MintUrl {
    type Err = url::ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url = s.parse::<Url>()?;
        Ok(url.into())
    }
}

impl AsRef<Url> for MintUrl {
    fn as_ref(&self) -> &Url {
        &self.raw
    }
}

impl fmt::Debug for MintUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.raw)
    }
}
impl fmt::Display for MintUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.raw)
    }
}

impl<'d> serde::Deserialize<'d> for MintUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'d>,
    {
        let url = Url::deserialize(deserializer)?;
        Ok(url.into())
    }
}

// https://8333.space:3338/ -> https://8333.space:3338
// trim ending / for url
impl serde::Serialize for MintUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let trim = self.as_str().trim_end_matches("/");
        serializer.serialize_str(trim)
    }
}

#[cfg(test)]
pub mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_mint_url_path_root() {
        let u = "https://8333.space:3338";
        let ur = "https://8333.space:3338/";

        for url in [u, ur] {
            let mp = MintProofs {
                proofs: vec![],
                mint: url.parse().unwrap(),
            };

            let js = serde_json::to_value(&mp).unwrap();
            println!("{}", js);
            assert_eq!(js["mint"].as_str().unwrap(), u);

            let js = serde_json::to_string(&mp).unwrap();
            println!("{}", js);
            let jsv: MintProofs = serde_json::from_str(&js).unwrap();
            assert_eq!(jsv.mint.as_str(), ur);
        }
    }

    #[test]
    fn test_mint_url_path_root_not() {
        let u = "https://mint.minibits.cash/Bitcoin";
        let ur = "https://mint.minibits.cash/Bitcoin/";

        for url in [u, ur] {
            let mp = MintProofs {
                proofs: vec![],
                mint: url.parse().unwrap(),
            };

            let js = serde_json::to_value(&mp).unwrap();
            println!("{}", js);
            assert_eq!(js["mint"].as_str().unwrap(), u);

            let js = serde_json::to_string(&mp).unwrap();
            println!("{}", js);
            let jsv: MintProofs = serde_json::from_str(&js).unwrap();
            assert_eq!(jsv.mint.as_str(), ur);
        }
    }

    #[test]
    fn test_deserialize_url() {
        for (i, url) in [
            "https://8333.space:3338/",
            "https://8333.space:3338//",
            "https://8333.space:3338/a",
            "https://8333.space:3338/a/",
            "https://8333.space:3338/.b",
        ]
        .into_iter()
        .enumerate()
        {
            let uri: Url = url.parse().unwrap();
            assert_eq!(uri.as_str(), url, "{}: {}->{}", i, url, uri.as_str())
        }

        for (i, url) in [
            "https://8333.space:3338",
            "https://8333.space:3338/.",
            "https://8333.space:3338/..",
        ]
        .into_iter()
        .enumerate()
        {
            let uri: Url = url.parse().unwrap();
            assert_ne!(uri.as_str(), url, "{}: {}->{}", i, url, uri.as_str())
        }
    }

    #[test]
    fn test_not_root_mint_url() {
        let u = "https://mint.minibits.cash/Bitcoin";
        let up: Url = u.parse().unwrap();
        let u2 = "https://mint.minibits.cash/Bitcoin/";
        let up2: Url = u2.parse().unwrap();
        assert_eq!(up.as_str(), u);
        assert_eq!(up2.as_str(), u2);
        assert_ne!(up.as_str(), up2.as_str());
    }

    #[test]
    fn test_token() {
        let v4 = r#"cashuBo2FteCJodHRwczovL21pbnQubWluaWJpdHMuY2FzaC9CaXRjb2luYXVjc2F0YXSBomFpSABQBVDwSUFGYXCCo2FhAmFzeEAxMGE5YjNlOWE5NmJmYjhlMGE2ZGJlMTY3YzA3YzhlYmUxYWQ0MjZhNTZmOGE4MjU4MDM2ODQ4ZmNlMzAzMDYxYWNYIQKVA4ylDqbmnxzWfDlVnrgvVxzDGCrQGjoHeMfrCsFFt6NhYQhhc3hAZGI5YjM5YjBkNDZhNWM0ZmY4ODc2OGRhNTI4MWE0ZmJmNjcyYzE1MTZiODU0NjE0OGU2NmI5N2NlYmQyY2RlOGFjWCED1RAJOBqPXGmpp0m1q5-MYiGf8s5q3klYdZ0PCcCfgiw"#;
        let t4 = v4.parse::<Token>().unwrap();
        let t3 = t4.clone().into_v3().unwrap();
        let v3 = t3.to_string();
        let t = v3.parse::<Token>().unwrap().into_v3().unwrap();
        assert_eq!(t3, t);

        let t4new = TokenV4::new(
            t.token[0].mint.clone(),
            &t.token[0].proofs,
            t.memo.clone(),
            t.unit,
        )
        .unwrap();
        let v42 = t4new.to_string();

        println!("{:?}", t4);
        println!("{:?}", t4new);
        assert_eq!(v4, v42);
    }
}
