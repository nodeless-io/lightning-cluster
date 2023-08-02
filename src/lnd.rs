use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Response;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;

use crate::cluster::{self, ClusterAddInvoice};

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
    pub value: u64,
    pub expiry: u64,
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
        LndClient {
            host,
            cert_path,
            macaroon_path,
        }
    }

    pub async fn new_address(&self) -> Result<NewAddressResponse> {
        let url = format!("{}/v1/newaddress", self.host);
        let response = LndClient::get(&url, &self.macaroon_path, &self.cert_path)
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
        let response = LndClient::post(&url, &body, &self.macaroon_path, &self.cert_path).await?;

        response
            .json::<AddInvoiceResponse>()
            .await
            .map_err(|error| anyhow::Error::from(error))
            .context("Failed to parse JSON response from LND API")
    }

    pub async fn lookup_invoice(&self, r_hash: &str) -> Result<LookupInvoiceResponse> {
        let r_hash = decode_and_convert_to_hex(&r_hash)?;

        let url = format!("{}/v1/invoice/{}", self.host, r_hash);
        let response = LndClient::get(&url, &self.macaroon_path, &self.cert_path).await?;

        response
            .json::<LookupInvoiceResponse>()
            .await
            .map_err(|error| anyhow::Error::from(error))
            .context("Failed to parse JSON response from LND API")
    }

    async fn get(url: &str, macaroon_path: &str, cert_path: &str) -> Result<Response> {
        let mut macaroon_data = Vec::new();
        let mut macaroon_file = fs::File::open(macaroon_path).unwrap();
        macaroon_file.read_to_end(&mut macaroon_data).unwrap();
        let macaroon_hex = hex::encode(macaroon_data);

        let mut headers = HeaderMap::new();
        headers.insert(
            "Grpc-Metadata-macaroon",
            HeaderValue::from_str(&macaroon_hex).unwrap(),
        );

        let mut buf = Vec::new();
        fs::File::open(cert_path)
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
        url: &str,
        body: &T,
        macaroon_path: &str,
        cert_path: &str,
    ) -> Result<Response> {
        let mut macaroon_data = Vec::new();
        let mut macaroon_file = fs::File::open(macaroon_path).unwrap();
        macaroon_file.read_to_end(&mut macaroon_data).unwrap();
        let macaroon_hex = hex::encode(macaroon_data);

        let mut headers = HeaderMap::new();
        headers.insert(
            "Grpc-Metadata-macaroon",
            HeaderValue::from_str(&macaroon_hex).unwrap(),
        );

        let mut buf = Vec::new();
        fs::File::open(cert_path)
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

fn decode_and_convert_to_hex(r_hash: &str) -> Result<String> {
    let decoded_bytes = base64::decode(r_hash)?;
    let hex_string = hex::encode(decoded_bytes);

    Ok(hex_string)
}
