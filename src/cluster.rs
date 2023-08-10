use crate::lnd::Route;
use crate::lnd::{AddInvoiceResponse, FeeLimit, LndClient, LndSendPaymentSyncReq};
use anyhow::Result;
use redis::aio::Connection;
use core::fmt;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
extern crate redis;
use redis::{AsyncCommands, FromRedisValue};

pub struct Cluster {
    pub nodes: Vec<Node>,
    pub cache: Connection,
    pub inv_exp_sec: i64,
    pub addr_exp_sec: i64,
    pub utxo_exp_sec: i64,
}

#[derive(Clone)]
pub struct Node {
    pub pubkey: String,
    pub ip: String,
    pub port: String,
    pub network: NodeNetwork,
    pub lightning_impl: NodeLightningImpl,
    pub client: NodeClient,
}

#[derive(Clone)]
pub enum NodeClient {
    Lnd(LndClient),
    CLightning,
    Eclair,
    Other,
}
#[derive(Clone)]
pub enum NodeNetwork {
    Mainnet,
    Testnet,
}

#[derive(Clone)]
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
    pub value: i64,
    pub expiry: i64,
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

impl FromRedisValue for ClusterLookupInvoice {
    fn from_redis_value(v: &redis::Value) -> redis::RedisResult<Self> {
        match v {
            redis::Value::Okay => {
                return Ok(ClusterLookupInvoice {
                    pubkey: "".to_string(),
                    memo: "".to_string(),
                    r_preimage: "".to_string(),
                    r_hash: "".to_string(),
                    value: "".to_string(),
                    settle_date: "".to_string(),
                    payment_request: "".to_string(),
                    description_hash: "".to_string(),
                    expiry: "".to_string(),
                    amt_paid_sat: "".to_string(),
                    state: ClusterInvoiceState::Open,
                })
            },
            redis::Value::Data(data) => {
                let json = String::from_utf8(data.to_vec()).unwrap();
                let invoice: ClusterLookupInvoice = serde_json::from_str(&json).unwrap();
                return Ok(invoice)
            },
            _ => panic!("Invalid redis value"),
        };
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClusterPayPaymentRequestRes {
    pub pubkey: String,
    pub payment_error: Option<String>,
    pub payment_preimage: Option<String>,
    pub payment_route: Option<Route>,
    pub payment_hash: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct ClusterUtxos {
    pub utxos: Vec<ClusterUtxo>,
}

impl FromRedisValue for ClusterUtxos {
    fn from_redis_value(v: &redis::Value) -> redis::RedisResult<Self> {
        match v {
            redis::Value::Okay => {
                return Ok(ClusterUtxos {
                    utxos: vec![],
                })
            },
            redis::Value::Data(data) => {
                let json = String::from_utf8(data.to_vec()).unwrap();
                let utxos: ClusterUtxos = serde_json::from_str(&json).unwrap();
                return Ok(utxos)
            },
            _ => panic!("Invalid redis value"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterUtxo {
    pub pubkey: String,
    pub address: String,
    pub amount: u64,
    pub confirmations: u64,
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

    pub async fn next_address(&self) -> Result<String> {
        match &self.client {
            NodeClient::Lnd(client) => {
                let addr = client.new_address().await?;
                Ok(addr.address)
            }
            _ => {
                panic!("We only support LND nodes at this time.")
            }
        }
    }

    pub async fn list_utxos(&self) -> Result<ClusterUtxos> {
        match &self.client {
            NodeClient::Lnd(client) => {
                let utxos = client.list_unspent().await?;
                let cluster_utxos = ClusterUtxos {
                    utxos: utxos
                        .utxos
                        .into_iter()
                        .map(|utxo| ClusterUtxo {
                            pubkey: self.pubkey.clone(),
                            address: utxo.address,
                            amount: utxo.amount_sat.parse::<u64>().unwrap(),
                            confirmations: utxo.confirmations.parse::<u64>().unwrap(),
                        })
                        .collect(),
                };
                Ok(cluster_utxos)
            }
            _ => {
                panic!("We only support LND nodes at this time.")
            }
        }
    }
}

impl Cluster {
    pub fn new(
        nodes: Vec<Node>,
        redis: redis::aio::Connection,
        inv_exp_sec: i64,
        addr_exp_sec: i64,
        utxo_exp_sec: i64,
    ) -> Cluster {
        Self {
            nodes,
            cache: redis,
            inv_exp_sec: inv_exp_sec,
            addr_exp_sec: addr_exp_sec,
            utxo_exp_sec: utxo_exp_sec,
        }
    }

    pub async fn lookup_invoice(
        &mut self,
        r_hash: &str,
        pubkey: Option<String>,
    ) -> Result<ClusterLookupInvoice> {
        let cached_invoice = self.cache.get(&r_hash.to_string()).await?;

        match cached_invoice {
            Some(invoice) => {
                eprintln!("cached");
                Ok(invoice)
            },
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
                    let json_string = serde_json::to_string(&hexed_invoice).unwrap();

                    let _: Result<ClusterLookupInvoice, _> = self.cache
                        .set_ex(
                            r_hash.to_string(),
                            json_string,
                            self.inv_exp_sec as usize,
                        ).await;
                        eprintln!("requested invoice from node");
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

                    let json_invoice = serde_json::to_string(&hexed_invoice).unwrap();

                    // Insert the successful result into the cache
                    let _: Result<ClusterLookupInvoice, _> = self.cache
                        .set_ex(
                            r_hash.to_string(),
                            json_invoice,
                            self.inv_exp_sec as usize,
                        )
                        .await;

                        eprintln!("requested invoice from node");

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

    pub async fn next_address(&mut self, pubkey: Option<String>) -> Result<String> {
        match pubkey {
            Some(pubkey) => {
                let node = self
                    .nodes
                    .iter()
                    .find(|node| node.pubkey == pubkey)
                    .unwrap();

                let addr = node.next_address().await?;

                let _: Result<String, _> = self.cache
                    .set_ex(
                        addr.clone(),
                        node.clone().pubkey,
                        self.addr_exp_sec as usize,
                    )
                    .await;
                Ok(addr)
            }
            None => {
                let mut rng = rand::thread_rng();
                let node = self.nodes.choose(&mut rng).unwrap();

                let addr = node.next_address().await?;

                let _: Result<String, _> = self.cache.set_ex(
                    addr.clone(),
                    node.clone().pubkey,
                    self.addr_exp_sec as usize,
                ).await;
                Ok(addr)
            }
        }
    }

    pub async fn list_utxos(&mut self, pubkey: Option<&str>) -> Result<ClusterUtxos> {
        match pubkey {
            Some(pubkey) => {
                let node = self
                    .nodes
                    .iter()
                    .find(|node| node.pubkey == pubkey)
                    .ok_or_else(|| anyhow::anyhow!("Node not found with provided pubkey"))?;

                let cache_key = format!("utxos:{}", node.pubkey);
                let cached_utxos = self.cache.get(&cache_key).await?;

                match cached_utxos {
                    Some(utxos) => Ok(utxos),
                    None => {
                        let utxos = node.list_utxos().await?;
                        let json_utxos = serde_json::to_string(&utxos).unwrap();
                        let _: Result<ClusterUtxos, _> = self.cache.set_ex(
                            cache_key,
                            json_utxos,
                            self.utxo_exp_sec as usize,
                        ).await;
                        Ok(utxos)
                    }
                }
            }
            None => {
                let mut cluster_utxos = ClusterUtxos { utxos: vec![] };

                for node in &self.nodes {
                    let cache_key = format!("utxos:{}", node.pubkey);
                    let cached_utxos = self.cache.get(&cache_key).await?;

                    let node_utxos = match cached_utxos {
                        Some(utxos) => utxos,
                        None => {
                            let fetched_utxos = node.list_utxos().await?;
                            let json_utxos = serde_json::to_string(&fetched_utxos).unwrap();
                            let _: Result<ClusterUtxos, _> = self.cache.set_ex(
                                cache_key,
                                json_utxos,
                                self.utxo_exp_sec as usize,
                            ).await;
                            fetched_utxos
                        }
                    };

                    cluster_utxos.utxos.extend(node_utxos.utxos);
                }

                Ok(cluster_utxos)
            }
        }
    }

    pub async fn pay_invoice(
        &self,
        amount: u64,
        payment_request: String,
        max_fee: i64,
        pubkey: Option<String>,
    ) -> Result<ClusterPayPaymentRequestRes> {
        // node selected
        if pubkey.is_some() {
            let node = self
                .nodes
                .iter()
                .find(|node| &node.pubkey == pubkey.as_ref().unwrap())
                .ok_or_else(|| anyhow::anyhow!("Node not found with provided pubkey"))?;

            match &node.client {
                NodeClient::Lnd(client) => {
                    let req = LndSendPaymentSyncReq {
                        payment_request: payment_request.clone(),
                        amt: amount.to_string(),
                        fee_limit: FeeLimit {
                            fixed: max_fee.to_string(),
                        },
                        allow_self_payment: false,
                    };
                    let invoice = client.send_payment_sync(req).await?;
                    eprintln!("{:?}", invoice);
                    Ok(invoice.to_cluster(node.clone().pubkey))
                }
                _ => {
                    panic!("We only support LND nodes at this time.")
                }
            }
        } else {
            // no node selected, select a node at random
            let mut rng = rand::thread_rng();
            let node = self.nodes.choose(&mut rng).unwrap();

            match &node.client {
                NodeClient::Lnd(client) => {
                    let req = LndSendPaymentSyncReq {
                        payment_request: payment_request.clone(),
                        amt: amount.to_string(),
                        fee_limit: FeeLimit {
                            fixed: max_fee.to_string(),
                        },
                        allow_self_payment: false,
                    };
                    let invoice = client.send_payment_sync(req).await?;
                    Ok(invoice.to_cluster(node.clone().pubkey))
                }
                _ => {
                    panic!("We only support LND nodes at this time.")
                }
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

#[cfg(test)]
pub mod tests {
    use crate::lnd::LndClient;

    use super::{Cluster, ClusterAddInvoice, Node, NodeClient, NodeLightningImpl, NodeNetwork};

    #[tokio::test]
    async fn test_add_lookup_invoice() {
        let mut cluster = create_test_cluster().await;
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

    pub async fn create_test_cluster() -> Cluster {
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
        let redis = redis::Client::open("redis://127.0.01/").unwrap().get_async_connection().await.unwrap();
        let cluster = Cluster::new(nodes, redis, 60, 60, 60);

        cluster
    }
}
