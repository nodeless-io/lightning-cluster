use crate::lnd::{LndClient, AddInvoiceResponse};
use anyhow::Result;
use core::fmt;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, ops::DerefMut};
use rand::seq::SliceRandom;
pub struct Cluster {
    pub nodes: Vec<Node>,
    pub next_node_index: usize,
}

pub struct Node {
    pub pubkey: String,
    pub ip: String,
    pub port: i16,
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

#[derive(Serialize, Deserialize, Debug)]
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

#[derive(Serialize, Deserialize, Debug)]
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
                            r_preimage: invoice.r_preimage,
                            r_hash: invoice.r_hash,
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
                panic!("Not implemented")
            }
        }
    }

    pub async fn add_invoice(self: &Self, req: ClusterAddInvoice) -> Result<AddInvoiceResponse> {
        match &self.client {
            NodeClient::Lnd(client) => {
                let response = client.add_invoice(req).await;

                match response {
                    Ok(invoice) => {
                        Ok(invoice)
                    }
                    Err(err) => Err(err),
                }
            }
            _ => {
                panic!("Not implemented")
            }
        }
    }
}

impl Cluster {
    pub fn new(nodes: Vec<Node>) -> Cluster {
        Cluster {
            nodes,
            next_node_index: 0,
        }
    }

    pub async fn lookup_invoice(
        &self,
        r_hash: &str,
        pubkey: Option<String>,
    ) -> Result<ClusterLookupInvoice> {
        if let Some(pubkey) = pubkey {
            let node = self
                .nodes
                .iter()
                .find(|node| node.pubkey == pubkey)
                .unwrap();
            node.lookup_invoice(r_hash).await
        } else {
            Err(anyhow::Error::msg("No pubkey provided"))
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
            },
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
        port: i16,
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
