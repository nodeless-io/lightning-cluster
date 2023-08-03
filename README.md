#Lightning Cluster

A load balanching and caching service for production lightning nodes.

```rust
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

    let get_invoice = cluster.lookup_invoice(&invoice.r_hash, None).await.unwrap();

    println!("{:?}", get_invoice);
}
```