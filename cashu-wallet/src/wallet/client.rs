pub use reqwest::Client as HttpClient;

use cashu::nuts::*;
use cashu::Amount;
use cashu::Bolt11Invoice;

use super::error::ClientError as Error;
use super::AmountHelper;
use super::BlindedMessages;
use super::MintUrl as Url;
use super::ProofsHelper;

use std::time::Duration;

pub static CURRENCY_UNIT_SAT: &str = "sat";
pub static PAYMEN_METHOD_BOLT11: &str = "bolt11";

/// <https://github.com/cashubtc/nuts/tree/main>
#[derive(Debug, Clone)]
pub struct MintClient {
    pub(super) url: Url,
    pub(super) http: HttpClient,
    pub(super) options: HttpOptions,
}

/// only used when could use
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct HttpOptions {
    #[serde(default)]
    pub connection_verbose: bool,
    pub timeout_connect_ms: Option<u64>,
    pub timeout_get_ms: Option<u64>,
    pub timeout_swap_ms: Option<u64>,
    pub timeout_melt_ms: Option<u64>,
}

impl HttpOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn connection_verbose(mut self, b: bool) -> Self {
        self.connection_verbose = b;
        self
    }

    pub fn timeout_connect_ms(mut self, millis: u64) -> Self {
        if millis > 0 {
            self.timeout_connect_ms = Some(millis);
        }
        self
    }
    pub fn timeout_connect(&self) -> Option<Duration> {
        self.timeout_connect_ms.map(Duration::from_millis)
    }

    pub fn timeout_get_ms(mut self, millis: u64) -> Self {
        if millis > 0 {
            self.timeout_get_ms = Some(millis);
        }
        self
    }
    pub fn timeout_get(&self) -> Option<Duration> {
        self.timeout_get_ms.map(Duration::from_millis)
    }

    pub fn timeout_swap_ms(mut self, millis: u64) -> Self {
        if millis > 0 {
            self.timeout_swap_ms = Some(millis);
        }
        self
    }
    pub fn timeout_split(&self) -> Option<Duration> {
        self.timeout_swap_ms.map(Duration::from_millis)
    }

    pub fn timeout_melt_ms(mut self, millis: u64) -> Self {
        if millis > 0 {
            self.timeout_melt_ms = Some(millis);
        }
        self
    }
    pub fn timeout_melt(&self) -> Option<Duration> {
        self.timeout_melt_ms.map(Duration::from_millis)
    }
}

impl MintClient {
    pub fn with_http(mint: Url, options: HttpOptions, http: HttpClient) -> Result<Self, Error> {
        Ok(Self {
            url: mint,
            http,
            options,
        })
    }

    pub fn new(mint: Url, options: HttpOptions) -> Result<Self, Error> {
        let mut h = HttpClient::builder().connection_verbose(options.connection_verbose);

        if let Some(t) = options.timeout_connect() {
            h = h.connect_timeout(t)
        }

        Ok(Self {
            http: h.build()?,
            url: mint,
            options,
        })
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    pub fn urlraw(&self) -> &url::Url {
        self.url.as_ref()
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    //  curl https://mint.host:3338/keys
    /// 01 	Mint public keys: Mint responds with his active keyset.
    // curl -X GET https://8333.space:3338/v1/keys
    // curl -X GET https://8333.space:3338/v1/keys/xxx
    pub async fn get_keys(&self, id: Option<&str>) -> Result<nut01::KeysResponse, Error> {
        let mut url = self.urlraw().join("v1/keys")?;
        if let Some(id) = id {
            url = self.urlraw().join(&format!("v1/keys/{id}"))?;
        }

        let mut req = self.http.get(url);
        if let Some(t) = self.options.timeout_get() {
            req = req.timeout(t);
        }

        let resp = req.send().await?;
        let httpcode = resp.status().as_u16() as i32;
        let body = resp.text().await?;

        Error::try_parse(&body, httpcode)
    }

    /*
    curl  https://8333.space:3338/keysets
    {"keysets":["I2yN+iRYfkzT"]}

    curl https://8333.space:3338/keys/I2yN+iRYfkzT
    {"1":"03ba786a2c0745f8c30e490288acd7a72dd53d65afd292ddefa326a4a3fa14c566","2":"03361cd8bd1329fea797a6add1cf1990ffcf2270ceb9fc81eeee0e8e9c1bd0cdf5"}
    */

    /// 02  keyset IDs
    // curl -X GET https://8333.space:3338/v1/keysets
    pub async fn get_keysetids(&self) -> Result<nut02::KeysetResponse, Error> {
        let url = self.urlraw().join("v1/keysets")?;

        let mut req = self.http.get(url);
        if let Some(t) = self.options.timeout_get() {
            req = req.timeout(t);
        }

        let resp = req.send().await?;
        let httpcode = resp.status().as_u16() as i32;
        let body = resp.text().await?;

        Error::try_parse(&body, httpcode)
    }

    /// NUT-03: Swap tokens
    pub async fn swap(
        &self,
        inputs: impl ProofsHelper,
        outputs: &BlindedMessages<'_>,
    ) -> Result<nut03::SwapResponse, Error> {
        let url = self.urlraw().join("v1/swap")?;

        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        pub struct Request<'a, T: serde::Serialize> {
            pub inputs: T,
            pub outputs: &'a BlindedMessages<'a>,
        }
        let request = Request {
            inputs: inputs.to_serde_raw(),
            outputs,
        };

        let mut req = self.http.post(url).json(&request);
        if let Some(t) = self.options.timeout_split() {
            req = req.timeout(t);
        }

        let resp = req.send().await?;
        let httpcode = resp.status().as_u16() as i32;
        let body = resp.text().await?;
        debug!("{}: {}", httpcode, body);

        Error::try_parse(&body, httpcode)
    }

    /// 04  Request minting
    pub async fn request_mint(
        &self,
        amount: Amount,
        unit: &str,
        method: &str,
    ) -> Result<nut04::MintQuoteBolt11Response, Error> {
        let mut url = self.urlraw().join("v1/mint/quote/")?;
        url = url.join(method)?;

        #[derive(Debug, Serialize)]
        pub struct Request<'a> {
            amount: u64,
            unit: &'a str,
        }

        let form = Request {
            amount: amount.to_u64(),
            unit,
        };

        let mut req = self.http.post(url).json(&form);
        if let Some(t) = self.options.timeout_get() {
            req = req.timeout(t);
        }

        let resp = req.send().await?;
        let httpcode = resp.status().as_u16() as i32;
        let body = resp.text().await?;

        debug!("{}: {}", httpcode, body);

        Error::try_parse(&body, httpcode)
    }

    /// 04 	Minting tokens
    pub async fn mint(
        &self,
        blinded_messages: &BlindedMessages<'_>,
        hash: &str,
        method: &str,
    ) -> Result<nut04::MintBolt11Response, Error> {
        let mut url = self.urlraw().join("v1/mint/")?;
        url = url.join(method)?;

        #[derive(Debug, Serialize)]
        pub struct Request<'a> {
            outputs: &'a BlindedMessages<'a>,
            quote: &'a str,
        }
        let request = Request {
            outputs: blinded_messages,
            quote: hash,
        };

        let mut req = self.http.post(url).json(&request);
        if let Some(t) = self.options.timeout_split() {
            req = req.timeout(t);
        }

        let resp = req.send().await?;

        // let resp = self.http.post(url).json(&request).send().await?;
        let httpcode = resp.status().as_u16() as i32;
        let body = resp.text().await?;

        debug!("{}: {}", httpcode, body);

        Error::try_parse(&body, httpcode)
    }

    /// 05 	Melting tokens: Melt quote
    /// https://github.com/cashubtc/nuts/blob/main/05.md
    pub async fn request_melt(
        &self,
        invoice: &Bolt11Invoice,
        unit: &str,
        method: &str,
    ) -> Result<nut05::MeltQuoteBolt11Response, Error> {
        let mut url = self.urlraw().join("v1/melt/quote/")?;
        url = url.join(method)?;

        #[derive(Debug, Serialize)]
        pub struct Request<'a> {
            request: &'a Bolt11Invoice,
            unit: &'a str,
        }
        let request = Request {
            request: invoice,
            unit,
        };

        let mut req = self.http.post(url).json(&request);
        if let Some(t) = self.options.timeout_get() {
            req = req.timeout(t);
        }

        let resp = req.send().await?;

        let httpcode = resp.status().as_u16() as i32;
        let body = resp.text().await?;

        debug!("{}: {}", httpcode, body);

        Error::try_parse(&body, httpcode)
    }

    /// 05 	Melting tokens
    ///
    /// NUT-08: Lightning fee return
    ///
    /// <https://github.com/cashubtc/nuts/blob/main/05.md#paying-the-invoice>
    ///
    /// ⚠️ Attention: This call will block until the Lightning payment either succeeds or fails. This can take quite a long time in case the Lightning payment is slow. Make sure to use no (or a very long) timeout when making this call!
    pub async fn melt(
        &self,
        inputs: impl ProofsHelper,
        quote: &str,
        outputs: Option<&BlindedMessages<'_>>,
        method: &str,
    ) -> Result<nut05::MeltBolt11Response, Error> {
        let mut url = self.urlraw().join("v1/melt/")?;
        url = url.join(method)?;

        // let request = nut05::CheckFeesRequest { pr: invoice };
        #[derive(Debug, Serialize)]
        pub struct Request<'a, T: serde::Serialize> {
            inputs: T,
            quote: &'a str,
            outputs: Option<&'a BlindedMessages<'a>>,
            // outputs: &'a Option<Vec<BlindedMessageSkip0>>,
        }
        // /// Blinded Message [NUT-00]
        // #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        // pub struct BlindedMessageSkip0 {
        //     /// Amount in satoshi
        //     #[serde(default, skip_serializing_if = "amount_is_zero")]
        //     pub amount: Amount,
        //     /// encrypted secret message (B_)
        //     #[serde(rename = "B_")]
        //     pub b: cashu::nuts::nut01::PublicKey,
        // }
        // fn amount_is_zero(amount: &Amount) -> bool {
        //     amount.to_sat() == 0
        // }

        // https://8333.space:3338:
        // 0 => 400: {"detail":"invalid amount: 0","code":10000}
        // null => 422: {"detail":[{"loc":["body","outputs",0,"amount"],"msg":"field required","type":"value_error.missing"}]}
        // let outputs = outputs.map(|ops| {
        //     ops.iter()
        //         .map(|o| BlindedMessageSkip0 {
        //             amount: o.amount,
        //             b: o.b.clone(),
        //         })
        //         .collect()
        // });

        // let outputs = outputs.map(|ops| {
        //     ops.iter()
        //         .map(|o| BlindedMessage {
        //             amount: 1.into(),
        //             b: o.b.clone(),
        //         })
        //         .collect()
        // });
        let request = Request {
            inputs: inputs.to_serde_raw(),
            outputs,
            quote,
        };
        // println!("{}", serde_json::to_string(&request).unwrap());

        let mut req = self.http.post(url).json(&request);
        if let Some(t) = self.options.timeout_melt() {
            req = req.timeout(t);
        }

        let resp = req.send().await?;
        let httpcode = resp.status().as_u16() as i32;
        let body = resp.text().await?;

        debug!("{}: {}", httpcode, body);

        Error::try_parse(&body, httpcode)
    }

    /// NUT-06: Mint information
    pub async fn get_info(&self) -> Result<crate::types::MintInfo, Error> {
        let url = self.urlraw().join("v1/info")?;

        let mut req = self.http.get(url);
        if let Some(t) = self.options.timeout_get() {
            req = req.timeout(t);
        }
        let resp = req.send().await?;

        let httpcode = resp.status().as_u16() as i32;
        let body = resp.text().await?;

        Error::try_parse(&body, httpcode)
    }

    /// 07 	Token state check: Spendable check
    /// https://github.com/cashubtc/nuts/blob/main/07.md
    pub async fn check_state(&self, ys: &[PublicKey]) -> Result<nut07::CheckStateResponse, Error> {
        let url = self.urlraw().join("v1/checkstate")?;

        /// Check spendabale request [NUT-07]
        #[derive(Debug, PartialEq, Eq, Serialize)]
        pub struct CheckStateRequest<'a> {
            #[serde(rename = "Ys")]
            pub ys: &'a [PublicKey],
        }

        let request = CheckStateRequest { ys };
        // println!("{}", serde_json::to_string(&request).unwrap());

        let mut req = self.http.post(url).json(&request);
        // maybe slow
        if let Some(t) = self.options.timeout_split() {
            req = req.timeout(t);
        }

        let resp = req.send().await?;
        let httpcode = resp.status().as_u16() as i32;
        let body = resp.text().await?;

        // info!("{}: {}", httpcode, body);

        Error::try_parse(&body, httpcode)
    }

    /// NUT-09: Restore signatures
    pub async fn restore(
        &self,
        blinded_messages: &BlindedMessages<'_>,
    ) -> Result<nut09::RestoreResponse, Error> {
        let url = self.urlraw().join("v1/restore")?;

        #[derive(Debug, Serialize)]
        pub struct Request<'a> {
            outputs: &'a BlindedMessages<'a>,
        }
        let request = Request {
            outputs: blinded_messages,
        };

        let mut req = self.http.post(url).json(&request);
        if let Some(t) = self.options.timeout_split() {
            req = req.timeout(t);
        }

        let resp = req.send().await?;

        // let resp = self.http.post(url).json(&request).send().await?;
        let httpcode = resp.status().as_u16() as i32;
        let body = resp.text().await?;

        debug!("{}: {}", httpcode, body);

        Error::try_parse(&body, httpcode)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_decode_error() {
        let err = r#"{"code":0,"error":"Lightning invoice not paid yet."}"#;

        let _error = Error::try_parse::<u32>(err, 200).unwrap_err();
    }
}
