use cluster::{ClusterAddInvoice, Node, NodeClient, NodeLightningImpl, NodeNetwork};
use lnd::LndClient;
use moka::future::Cache;
use std::time::Duration;
use tokio::main;

use crate::cluster::ClusterLookupInvoice;

mod cluster;
mod lnd;

#[tokio::main]
async fn main() {
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
    let cluster = cluster::Cluster::new(nodes);

    let req = ClusterAddInvoice {
        pubkey: None,
        memo: String::from("test"),
        value: 1000,
        expiry: 1000,
    };

    let invoice = cluster.add_invoice(req, None).await.unwrap();

    let mut i = 0;

    loop {
        // measure how long each loop takes
        let start = std::time::Instant::now();
        let get_invoice = cluster
        .lookup_invoice(&invoice.r_hash, None)
        .await
        .unwrap();

        i = i + 1;

        let elapsed = start.elapsed();

        if elapsed > Duration::from_nanos(10000) {
            break;
        }

        println!("cache hits: {}", i);

    }
}
