use crate::lnd::{AddInvoiceResponse, LndClient};
use anyhow::Result;
use core::fmt;
use moka::future::Cache;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, time::Duration};
use crate::lnd::Route;

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

pub struct ClusterPayPaymentRequestRes {
    pub payment_error: Option<String>,
    pub payment_preimage: Option<String>,
    pub payment_route: Option<Route>,
    pub payment_hash: Option<String>,
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
                let invoice = client.lookup_invoice(r_hash).await?;
                Ok(invoice.to_cluster(&self.pubkey))
            }
            _ => {
                panic!("We only support LND nodes at this time.")
            }
        }
    }

    pub async fn add_invoice(self: &Self, req: ClusterAddInvoice) -> Result<AddInvoiceResponse> {
        match &self.client {
            NodeClient::Lnd(client) => {
                let invoice = client.add_invoice(req).await?;

                        let response = AddInvoiceResponse {
                            r_hash: to_hex(&invoice.r_hash)?,
                            payment_addr: to_hex(&invoice.payment_addr)?,
                            ..invoice
                        };
                        Ok(response)
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
                    let hexed_invoice = ClusterLookupInvoice {
                        r_hash: to_hex(&invoice.r_hash)?,
                        r_preimage: to_hex(&invoice.r_preimage)?,
                        ..invoice
                    };
                    self.invoice_cache
                        .insert(r_hash.to_string(), hexed_invoice.clone())
                        .await;
                    Ok(hexed_invoice)
                } else {
                    // Make calls to all nodes to find who owns the invoice
                    let mut tasks = vec![];
                    for node in &self.nodes {
                        let r_hash_clone = r_hash.clone();
                        let task = node.lookup_invoice(r_hash_clone);
                        tasks.push(task);
                    }

                    // Wait for all tasks to complete and find the first successful result
                    let success_result = match futures::future::join_all(tasks)
                        .await
                        .into_iter()
                        .enumerate()
                        .find_map(|(_index, result)| result.ok())
                    {
                        Some(success_result) => success_result,
                        None => return Err(anyhow::Error::msg("No nodes found this invoice.")),
                    };

                    let hexed_invoice = ClusterLookupInvoice {
                        r_hash: to_hex(&success_result.r_hash)?,
                        r_preimage: to_hex(&success_result.r_preimage)?,
                        ..success_result.clone()
                    };

                    // Insert the successful result into the cache
                    self.invoice_cache
                        .insert(r_hash.to_string(), hexed_invoice.clone())
                        .await;

                    Ok(hexed_invoice)
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

    // pub async fn pay_invoice(
    //     &self, 
    //     amount: u64, 
    //     payment_request: String, 
    //     pubkey: Option<String>) -> Result<ClusterPayPaymentRequestRes> {
        
    //     // node selected
    //     if pubkey.is_some() {
    //         let node = self
    //             .nodes
    //             .iter()
    //             .find(|node| node.pubkey == pubkey.unwrap())
    //             .unwrap();
    //         match &node.client {
    //             NodeClient::Lnd(client) => {
    //                 let req = LndSendPaymentSyncReq {
    //                     payment_request: payment_request.clone(),
    //                     amt: amount.to_string(),
    //                     fee_limit: FeeLimit {
    //                         fixed: String::from("0"),
    //                     },
    //                     allow_self_payment: false,
    //                 };
    //                 let invoice = client.send_payment_sync(req).await?;
    //                 Ok(invoice.to_cluster())
    //             }
    //             _ => {
    //                 panic!("We only support LND nodes at this time.")
    //             }
    //         }
    //     } else {
    //         // no node selected
    //     }
    // }

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

#[cfg(test)]
pub mod tests {
    use crate::lnd::LndClient;

    use super::{Cluster, Node, NodeNetwork, NodeLightningImpl, NodeClient, ClusterAddInvoice};


    #[tokio::test]
    async fn test_add_lookup_invoice() {
        let cluster = create_test_cluster();
        let add_invoice = ClusterAddInvoice {
            pubkey: None,
            memo: String::from("test"),
            value: 1000,
            expiry: 1000,
        };
        let invoice = cluster.add_invoice(add_invoice, None).await.unwrap();

        assert_eq!(invoice.r_hash.len(), 64);
        assert_eq!(invoice.payment_addr.len(), 64);

        let lookup_invoice = cluster.lookup_invoice(&invoice.r_hash, None).await.unwrap();

        assert_eq!(lookup_invoice.r_hash, invoice.r_hash);
    }

    pub fn create_test_cluster() -> Cluster {
        let node1 = Node {
            pubkey: dotenvy::var("NODE1_PUBKEY").unwrap(),
            ip: dotenvy::var("NODE1_IP").unwrap(),
            port: dotenvy::var("NODE1_PORT").unwrap(),
            network: NodeNetwork::Testnet,
            lightning_impl: NodeLightningImpl::Lnd,
            client: NodeClient::Lnd(LndClient::new(
                dotenvy::var("NODE1_HOST").unwrap(),
                dotenvy::var("NODE1_CERT_PATH").unwrap(),
                dotenvy::var("NODE1_MACAROON_PATH").unwrap(),
            )),
        };
    
        let nodes = vec![node1];
        let cluster = Cluster::new(nodes);

        cluster
    }
}