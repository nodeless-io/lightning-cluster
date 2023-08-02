use crate::lnd::{AddInvoiceResponse, LndClient};
use anyhow::Result;
use core::fmt;
use moka::future::Cache;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, time::Duration};
use tokio::join;
use futures::future::select_ok;

pub struct Cluster {
    pub nodes: Vec<Node>,
    pub next_node_index: usize,
    pub invoice_cache: Cache<String, ClusterLookupInvoice>,
}

pub struct Node {
    pub pubkey: String,
    pub ip: String,
    pub port: String,
    pub network: NodeNetwork,
    pub lightning_impl: NodeLightningImpl,
    pub client: NodeClient,
}

pub enum NodeClient {
    Lnd(LndClient),
    CLightning,
    Eclair,
    Other,
}
pub enum NodeNetwork {
    Mainnet,
    Testnet,
}

pub enum NodeLightningImpl {
    Lnd,
    CLightning,
    Eclair,
    Other,
}

pub struct ClusterError {
    pub message: String,
}

impl Display for ClusterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ClusterError: {}", self.message)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClusterAddInvoice {
    pub pubkey: Option<String>,
    pub memo: String,
    pub value: u64,
    pub expiry: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClusterLookupInvoice {
    pub pubkey: String,
    pub memo: String,
    pub r_preimage: String,
    pub r_hash: String,
    pub value: String,
    pub settle_date: String,
    pub payment_request: String,
    pub description_hash: String,
    pub expiry: String,
    pub amt_paid_sat: String,
    pub state: ClusterInvoiceState,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClusterInvoiceState {
    #[serde(rename = "OPEN")]
    Open = 0,
    #[serde(rename = "SETTLED")]
    Settled = 1,
    #[serde(rename = "CANCELED")]
    Canceled = 2,
    #[serde(rename = "ACCEPTED")]
    Accepted = 3,
}

impl Node {
    pub async fn lookup_invoice(self: &Self, r_hash: &str) -> Result<ClusterLookupInvoice> {
        match &self.client {
            NodeClient::Lnd(client) => {
                let response = client.lookup_invoice(r_hash).await;

                match response {
                    Ok(invoice) => {
                        let cluster_invoice = ClusterLookupInvoice {
                            pubkey: self.pubkey.clone(),
                            memo: invoice.memo,
                            r_preimage: to_hex(&invoice.r_preimage)?,
                            r_hash: String::from(r_hash),
                            value: invoice.value,
                            settle_date: invoice.settle_date,
                            payment_request: invoice.payment_request,
                            description_hash: invoice.description_hash,
                            expiry: invoice.expiry,
                            amt_paid_sat: invoice.amt_paid_sat,
                            state: invoice.state.to_cluster(),
                        };

                        Ok(cluster_invoice)
                    }
                    Err(err) => Err(err),
                }
            }
            _ => {
                panic!("We only support LND nodes at this time.")
            }
        }
    }

    pub async fn add_invoice(self: &Self, req: ClusterAddInvoice) -> Result<AddInvoiceResponse> {
        match &self.client {
            NodeClient::Lnd(client) => {
                let response = client.add_invoice(req).await;

                match response {
                    Ok(invoice) => {
                        let response = AddInvoiceResponse {
                            r_hash: to_hex(&invoice.r_hash)?,
                            payment_addr: to_hex(&invoice.payment_addr)?,
                            ..invoice
                        };
                        Ok(response)
                    }
                    Err(err) => Err(err),
                }
            }
            _ => {
                panic!("We only support LND nodes at this time.")
            }
        }
    }
}

impl Cluster {
    pub fn new(nodes: Vec<Node>) -> Cluster {
        let invoice_cache: Cache<String, ClusterLookupInvoice> = Cache::builder()
            .time_to_live(Duration::from_secs(3))
            .build();

        Self {
            nodes,
            next_node_index: 0,
            invoice_cache,
        }
    }

    pub async fn lookup_invoice(
        &self,
        r_hash: &str,
        pubkey: Option<String>,
    ) -> Result<ClusterLookupInvoice> {
        let cached_invoice = self.invoice_cache.get(&r_hash.to_string());
    
        match cached_invoice {
            Some(invoice) => Ok(invoice),
            None => {
                if let Some(pubkey) = pubkey {
                    let node = self
                        .nodes
                        .iter()
                        .find(|node| node.pubkey == pubkey)
                        .unwrap();
                    let invoice = node.lookup_invoice(r_hash).await?;
                    self.invoice_cache.insert(r_hash.to_string(), invoice.clone()).await;
                    Ok(invoice)
                } else {
                    // Make calls to all nodes to find who owns the invoice
                    let mut tasks = vec![];
                    for node in &self.nodes {
                        let r_hash_clone = r_hash.clone();
                        let task = node.lookup_invoice(r_hash_clone);
                        tasks.push(task);
                    }
    
                    // Wait for all tasks to complete and find the first successful result
                    let success_result = match futures::future::join_all(tasks).await.into_iter().enumerate().find_map(|(index, result)| result.ok()) {
                        Some(success_result) => success_result,
                        None => return Err(anyhow::Error::msg("No nodes found this invoice.")),
                    };
    
                    // Insert the successful result into the cache
                    self.invoice_cache
                        .insert(r_hash.to_string(), success_result.clone())
                        .await;
                    
                    Ok(success_result)
                }
            }
        }
    }
    

    pub async fn add_invoice(
        &self,
        req: ClusterAddInvoice,
        pubkey: Option<String>,
    ) -> Result<AddInvoiceResponse> {
        match pubkey {
            Some(pubkey) => {
                let node = self
                    .nodes
                    .iter()
                    .find(|node| node.pubkey == pubkey)
                    .unwrap();
                node.add_invoice(req).await
            }
            None => {
                let mut rng = rand::thread_rng();
                let node = self.nodes.choose(&mut rng).unwrap();
                node.add_invoice(req).await
            }
        }
    }
}

impl Node {
    pub fn new(
        pubkey: String,
        ip: String,
        port: String,
        network: NodeNetwork,
        lightning_impl: NodeLightningImpl,
        client: NodeClient,
    ) -> Node {
        Self {
            pubkey,
            ip,
            port,
            network,
            lightning_impl,
            client,
        }
    }
}

impl Display for NodeNetwork {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NodeNetwork::Mainnet => write!(f, "mainnet"),
            NodeNetwork::Testnet => write!(f, "testnet"),
        }
    }
}

pub fn to_hex(str: &str) -> Result<String> {
    let decoded_bytes = base64::decode(str)?;
    let hex_string = hex::encode(decoded_bytes);

    Ok(hex_string)
}
