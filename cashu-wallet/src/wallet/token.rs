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

pub type Token = TokenGeneric<Proofs>;
// pub type TokenRef = TokenGeneric<[Proof]>;
pub type TokenExtened = TokenGeneric<ProofsExtended>;
// pub type TokenExtenedRef = TokenGeneric<[ProofExtended]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenGeneric<T: ProofsHelper> {
    pub token: Vec<MintProofsGeneric<T>>,
    /// Memo for token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    /// Token Unit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<CurrencyUnit>,
}

impl<T: ProofsHelper> TokenGeneric<T> {
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

impl std::str::FromStr for Token {
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
        let token: Token = serde_json::from_str(&decoded_str)?;
        Ok(token)
    }
}

use std::fmt;

impl<T: ProofsHelper> fmt::Display for TokenGeneric<T> {
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

/// wrap Url for compat all
#[derive(Clone, PartialEq, Eq)]
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
}
