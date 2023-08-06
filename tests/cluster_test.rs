mod tests {
    use lightning_cluster::{cluster::{Node, NodeNetwork, NodeLightningImpl, NodeClient, ClusterAddInvoice, Cluster}, lnd::LndClient};

    #[tokio::test]
    async fn test_lightning_cluster() {
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

        let req = ClusterAddInvoice {
            pubkey: None,
            memo: String::from("test"),
            value: 1000,
            expiry: 1000,
        };

        let invoice = cluster.add_invoice(req, None).await.unwrap();
        let get_invoice = cluster.lookup_invoice(&invoice.r_hash, None).await.unwrap();

        println!("{:?}", get_invoice);

        let next_addr = cluster.next_address(None).await.unwrap();

        println!("{:?}", next_addr);

        let utxos = cluster.list_utxos(None).await.unwrap();

        for utxo in utxos.utxos {
            println!("{:?}", utxo);
        }

        let payment_request = String::from("lntb10u1pjva6sepp5lqz5lysxd7vu7h3nqzj3lem544uqmvec5k53cp2msm2lvnw0s9zqdqqcqzzsxqr23ssp5dysff7u8n2w7f0x5gysmlze7zw3fg05f2e2q24tzh8vanfnt5nss9qyyssqtcashms9q6dmt4ywja8jrtkztzr5kr5k24wa8mdxs00fgxq76d9zvs6styvhuxc5pvdcrs4m89r4rmvkp6lvc7tr959cds7na7k63vcplqfzxx");

        let _ = cluster.pay_invoice(1000, payment_request, 100, None).await.unwrap();

        //println!("{:?}", pay_ln_invoice);
    }
}