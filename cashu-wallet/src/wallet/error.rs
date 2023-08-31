use std::fmt;
use strum::EnumIs;

use serde::{Deserialize, Serialize};

#[derive(Debug)]
//
#[derive(EnumIs, thiserror::Error)]
pub enum WalletError {
    /// mint url unmatched
    #[error("Mint url unmatched")]
    MintUrlUnmatched,
    #[error("{0}")]
    Cashu(#[from] cashu::nuts::nut00::Error),
    /// mint client returns
    #[error("{0}")]
    Client(#[from] ClientError),
    /// custum error
    #[error("{0}")]
    Custom(#[from] anyhow::Error),
    #[error("Insufficant Funds")]
    InsufficientFunds,
    // /// Proofs required
    // #[error("Proofs required in token")]
    // ProofsRequired,
    // /// Unsupported token
    // #[error("Unsupported token")]
    // UnsupportedToken,
}

impl From<cashu::error::Error> for WalletError {
    fn from(value: cashu::error::Error) -> Self {
        Self::Cashu(value.into())
    }
}

impl WalletError {
    pub fn insufficant_funds() -> Self {
        Self::InsufficientFunds
    }
    pub fn is_outputs_already_signed_before(&self) -> bool {
        // 11000 outputs have already been signed before.
        if let WalletError::Client(c) = self {
            return c.is_outputs_already_signed_before();
        }
        false
    }
}

#[derive(Debug)]
//
#[derive(EnumIs)]
pub enum ClientError {
    /// Url Error
    Url(url::ParseError),
    /// Json error
    Json(serde_json::Error),
    /// reqwest error
    Reqwest(reqwest::Error),
    /// mint returns Error: <code, detail/error>
    Mint(i32, String),
    /// unknown http response
    UnknownResponse(i32, String),
}

impl ClientError {
    pub fn is_outputs_already_signed_before(&self) -> bool {
        // 11000 outputs have already been signed before.
        if let ClientError::Mint(c, d) = self {
            if *c == 11000 || d.contains("outputs have already been signed before") {
                return true;
            }
        }
        false
    }
}

impl From<url::ParseError> for ClientError {
    fn from(err: url::ParseError) -> ClientError {
        Self::Url(err)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(err: serde_json::Error) -> ClientError {
        Self::Json(err)
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(e: reqwest::Error) -> ClientError {
        Self::Reqwest(e)
    }
}

impl std::error::Error for ClientError {}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ClientError::*;

        match &self {
            Url(err) => write!(f, "{}", err),
            Json(err) => write!(f, "{}", err),
            Reqwest(err) => write!(f, "{}", err),
            Mint(code, err) => write!(f, "{} {}", code, err),
            UnknownResponse(code, body) => {
                write!(f, "mint returns unknown response(code: {}): {}", code, body)
            }
        }
    }
}

// erros has not NUT now,
// 0.13 become detail, before is error
// https://github.com/cashubtc/cashu/blob/main/cashu/core/errors.py#L38
// go is error now
// https://github.com/cashubtc/cashu-feni/blob/v0.2.0/cashu/cashu.go#L81
//
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MintErrorResponse {
    code: i32,
    error: Option<String>,
    detail: Option<String>,
}

impl ClientError {
    pub fn from_body(body: &str) -> Result<Self, anyhow::Error> {
        let mut json: MintErrorResponse = serde_json::from_str(body)?;

        let detail = json.detail.take().or_else(|| json.error.take());

        let e = Self::Mint(json.code, detail.unwrap_or_else(|| body.to_owned()));

        Ok(e)
    }

    pub fn try_parse<T: serde::de::DeserializeOwned>(body: &str, httpcode: i32) -> Result<T, Self> {
        let js = serde_json::from_str::<T>(body);

        match js {
            Ok(res) => Ok(res),
            Err(_) => {
                let e = Self::from_body(body)
                    .map_err(|_| Self::UnknownResponse(httpcode, body.to_owned()))?;
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ClientError::*;
    #[test]
    fn test_deserialize_error() -> anyhow::Result<()> {
        let input = "{\"code\":0,\"error\":\"Lightning invoice not paid yet.\"}";
        let data = ClientError::from_body(input)?;
        let data = match data {
            Mint(code, desc) => (code, desc),
            _ => panic!("{}", data),
        };

        assert_eq!(data.0, 0);
        assert_eq!(data.1, "Lightning invoice not paid yet.");
        Ok(())
    }

    #[test]
    fn test_deserialize_error_receive() -> anyhow::Result<()> {
        let input = r#"{"detail":"Token already spent.","code":11001}"#;

        let data = ClientError::from_body(input)?;
        let data = match data {
            Mint(code, desc) => (code, desc),
            _ => panic!("{}", data),
        };
        assert_eq!(data.0, 11001);
        assert_eq!(data.1, "Token already spent.");
        Ok(())
    }
}
