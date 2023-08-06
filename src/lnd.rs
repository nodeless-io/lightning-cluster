use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Response;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use crate::cluster::{self, ClusterAddInvoice, ClusterUtxos, ClusterUtxo};

#[derive(Clone)]
pub struct LndClient {
    pub host: String,
    pub cert_path: String,
    pub macaroon_path: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct NewAddressResponse {
    pub address: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AddInvoiceLndRequest {
    pub memo: String,
    pub value: i64,
    pub expiry: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListUnspentRequest {
    pub min_confs: i64,
    pub max_confs: i64,
    pub account: Option<String>,
    pub unconfirmed_only: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListUnspentResponse {
    pub utxos: Vec<Utxo>,
}

impl ListUnspentResponse {
    pub fn to_cluster(self, pubkey: String) -> Result<ClusterUtxos> {
        let mut utxos = Vec::new();
        for utxo in self.utxos {
            utxos.push(utxo.to_cluster(pubkey.clone())?);
        }

        Ok(ClusterUtxos {
            utxos: utxos,
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Utxo {
    pub address: String,
    pub amount_sat: String,
    pub confirmations: String,
    pub outpoint: Outpoint,
    pub pk_script: String,
}

impl Utxo {
    pub fn to_cluster(self, pubkey: String) -> Result<ClusterUtxo> {
        let amount = self.amount_sat.parse::<u64>()?;
        Ok(ClusterUtxo {
            pubkey: pubkey,
            address: self.address,
            amount: amount,
            confirmations: self.confirmations.parse::<u64>()?,
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Outpoint {
    pub txid_bytes: String,
    pub txid_str: String,
    pub output_index: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AddInvoiceResponse {
    pub r_hash: String,
    pub payment_request: String,
    pub add_index: String,
    pub payment_addr: String,
}

#[derive(Deserialize, Debug)]
pub struct LookupInvoiceResponse {
    pub memo: String,
    pub r_preimage: String,
    pub r_hash: String,
    pub value: String,
    pub settle_date: String,
    pub payment_request: String,
    pub description_hash: String,
    pub expiry: String,
    pub amt_paid_sat: String,
    pub state: InvoiceState,
}

impl LookupInvoiceResponse {
    pub fn to_cluster(self, pubkey: &str) -> cluster::ClusterLookupInvoice {
        let state = self.state.to_cluster();
        cluster::ClusterLookupInvoice {
            pubkey: pubkey.to_string(),
            memo: self.memo,
            r_preimage: self.r_preimage,
            r_hash: self.r_hash,
            value: self.value,
            settle_date: self.settle_date,
            payment_request: self.payment_request,
            description_hash: self.description_hash,
            expiry: self.expiry,
            amt_paid_sat: self.amt_paid_sat,
            state: state,
        }
    }
}

#[derive(Deserialize, Debug)]
pub enum InvoiceState {
    #[serde(rename = "OPEN")]
    Open = 0,
    #[serde(rename = "SETTLED")]
    Settled = 1,
    #[serde(rename = "CANCELED")]
    Canceled = 2,
    #[serde(rename = "ACCEPTED")]
    Accepted = 3,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct LndSendPaymentSyncReq {
    pub payment_request: String,
    pub amt: String,
    pub fee_limit: FeeLimit,
    pub allow_self_payment: bool,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct FeeLimit {
    pub fixed: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LndSendPaymentSyncRes {
    pub payment_error: Option<String>,
    pub payment_preimage: Option<String>,
    pub payment_route: Option<Route>,
    pub payment_hash: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Route {
    pub total_time_lock: u64,
    pub total_fees: String,
    pub total_amt: String,
    pub hops: Vec<Hop>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Hop {
    pub chan_id: String,
    pub chan_capacity: String,
    pub amt_to_forward: String,
    pub fee: String,
    pub expiry: i64,
    pub amt_to_forward_msat: String,
    pub fee_msat: String,
    pub pub_key: String,
    pub metadata: String,
}

impl LndSendPaymentSyncRes {
    pub fn to_cluster(self, pubkey: String) -> cluster::ClusterPayPaymentRequestRes {
        cluster::ClusterPayPaymentRequestRes {
            pubkey: pubkey,
            payment_error: self.payment_error,
            payment_preimage: self.payment_preimage,
            payment_route: self.payment_route,
            payment_hash: self.payment_hash,
        }
    }
}

impl InvoiceState {
    pub fn to_cluster(&self) -> cluster::ClusterInvoiceState {
        match self {
            InvoiceState::Open => cluster::ClusterInvoiceState::Open,
            InvoiceState::Settled => cluster::ClusterInvoiceState::Settled,
            InvoiceState::Canceled => cluster::ClusterInvoiceState::Canceled,
            InvoiceState::Accepted => cluster::ClusterInvoiceState::Accepted,
        }
    }
}

impl LndClient {
    pub fn new(host: String, cert_path: String, macaroon_path: String) -> LndClient {
        Self {
            host,
            cert_path,
            macaroon_path,
        }
    }

    pub async fn new_address(&self) -> Result<NewAddressResponse> {
        let url = format!("{}/v1/newaddress", self.host);
        let response = LndClient::get(&self, &url)
            .await
            .map_err(|error| anyhow::Error::from(error))
            .context("Failed to make request to LND API")?;

        response
            .json::<NewAddressResponse>()
            .await
            .map_err(|error| anyhow::Error::from(error))
            .context("Failed to parse JSON response from LND API")
    }

    pub async fn add_invoice(&self, req: ClusterAddInvoice) -> Result<AddInvoiceResponse> {
        let url = format!("{}/v1/invoices", self.host);
        let body = AddInvoiceLndRequest {
            memo: req.memo,
            value: req.value,
            expiry: req.expiry,
        };
        let response = LndClient::post(&self, &url, &body).await?;

        response
            .json::<AddInvoiceResponse>()
            .await
            .map_err(|error| anyhow::Error::from(error))
            .context("Failed to parse JSON response from LND API")
    }

    pub async fn lookup_invoice(&self, r_hash: &str) -> Result<LookupInvoiceResponse> {
        let url = format!("{}/v1/invoice/{}", self.host, r_hash);
        let response = LndClient::get(&self, &url).await?;

        response
            .json::<LookupInvoiceResponse>()
            .await
            .map_err(|error| anyhow::Error::from(error))
            .context("Failed to parse JSON response from LND API")
    }

    pub async fn send_payment_sync(&self, req: LndSendPaymentSyncReq) -> Result<LndSendPaymentSyncRes> {
        let url = format!("{}/v1/channels/transactions", self.host);
        let res = LndClient::post(&self, &url, &req).await.unwrap();

        let json_string = res.text().await.unwrap();

        eprintln!("{}", json_string);

        let json = serde_json::from_str::<serde_json::Value>(&json_string).unwrap();

        let payment_hash = match &json["payment_hash"] {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) if s.is_empty() => None,
            serde_json::Value::String(s) => Some(to_hex(&s)?),
            _ => None, 
        };
        
        let payment_error = match &json["payment_error"] {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) if s.is_empty() => None,
            serde_json::Value::String(s) => Some(s.clone()),
            _ => None, 
            };

        let payment_route = match &json["payment_route"] {
            serde_json::Value::Null => None,
            _ => {
                let route = serde_json::to_string(&json["payment_route"]).unwrap();
                let route = serde_json::from_str::<Route>(&route).unwrap();
                Some(route)
            }
        };

        let payment_preimage = match &json["payment_preimage"] {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) if s.is_empty() => None,
            serde_json::Value::String(s) => Some(to_hex(&s)?),
            _ => None,
        };

        let res = LndSendPaymentSyncRes {
            payment_error,
            payment_preimage,
            payment_route,
            payment_hash,
        };

        eprintln!("{:?}", res);

        Ok(res)
    }

    pub async fn list_unspent(&self) -> Result<ListUnspentResponse> {
        let url = format!("{}/v2/wallet/utxos", self.host);

        let req = ListUnspentRequest {
            min_confs: 0,
            max_confs: 500,
            account: None,
            unconfirmed_only: None,
        };
        let response = LndClient::post(&self, &url, &req).await?;

        let json = response
            .json::<ListUnspentResponse>()
            .await
            .map_err(|error| anyhow::Error::from(error))?;

        Ok(json)
    }

    async fn get(&self, url: &str) -> Result<Response> {
        let mut macaroon_data = Vec::new();
        let mut macaroon_file = fs::File::open(&self.macaroon_path).unwrap();
        macaroon_file.read_to_end(&mut macaroon_data).unwrap();
        let macaroon_hex = hex::encode(macaroon_data);

        let mut headers = HeaderMap::new();
        headers.insert(
            "Grpc-Metadata-macaroon",
            HeaderValue::from_str(&macaroon_hex).unwrap(),
        );

        let mut buf = Vec::new();
        fs::File::open(&self.cert_path)
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        let cert = reqwest::Certificate::from_pem(&buf).unwrap();

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .add_root_certificate(cert)
            .build()
            .unwrap();

        let resp = client.get(url).send().await?;

        Ok(resp)
    }

    async fn post<T: serde::Serialize>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<Response> {
        let mut macaroon_data = Vec::new();
        let mut macaroon_file = fs::File::open(&self.macaroon_path).unwrap();
        macaroon_file.read_to_end(&mut macaroon_data).unwrap();
        let macaroon_hex = hex::encode(macaroon_data);

        let mut headers = HeaderMap::new();
        headers.insert(
            "Grpc-Metadata-macaroon",
            HeaderValue::from_str(&macaroon_hex).unwrap(),
        );

        let mut buf = Vec::new();
        fs::File::open(&self.cert_path)
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        let cert = reqwest::Certificate::from_pem(&buf).unwrap();

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .add_root_certificate(cert)
            .build()
            .unwrap();

        let resp = client.post(url).json(body).send().await?;

        Ok(resp)
    }
}

pub fn to_hex(str: &str) -> Result<String> {
    let decoded_bytes = base64::decode(str)?;
    let hex_string = hex::encode(decoded_bytes);

    Ok(hex_string)
}

#[cfg(test)]
mod tests {
    use crate::lnd::{LndClient, LndSendPaymentSyncReq, FeeLimit};

    #[tokio::test]
    async fn test_send_payment_sync() {
        let client = LndClient::new(
            dotenvy::var("NODE1_HOST").unwrap(),
            dotenvy::var("NODE1_CERT_PATH").unwrap(),
            dotenvy::var("NODE1_MACAROON_PATH").unwrap(),
        );

        // can't self pay invoices, hardcoding for now, invoice already paid.
        let payment_request = String::from("lntb10u1pjv4fjnpp5vnx7xwnqmaceg3kkeayhq7yk4zp7ppdvakdfuxj959k7d3s5gzmqdqqcqzzsxqr23ssp5vjnsq8jy5fw8ynq842ta8lppf4esh72m4mn79z46jxf93ncw7gus9qyyssqterg9uuet8uzqt63ehwha5pdv2ted8r2f8u4s35lg5yedrfutvkqjfxyf76zaskmycn9m05vnjy6ctytluxn639u2qdtydzzzn09r4qpv6uahm");

        let payment_req = LndSendPaymentSyncReq {
            payment_request: payment_request,
            amt: String::from("1000"),
            fee_limit: FeeLimit {
                fixed: 10.to_string(),
            },
            allow_self_payment: true,
        };
        
        let payment = client.send_payment_sync(payment_req).await;

        eprintln!("{:?}", payment);
    }
}
