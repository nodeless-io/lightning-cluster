use std::process::exit;
use std::sync::Arc;

use actix_web::web::Data;
use actix_web::{App, HttpServer};
use api::{status_handler, lookup_invoice_handler, add_invoice_handler};

mod api;
mod cluster;

use cluster::{Cluster, Node, NodeLightningImpl, NodeNetwork};
use lnd::LndClient;
use reqwest::header::{HeaderMap, HeaderValue};
use std::fs;
use std::io::Read;
use tokio::sync::Mutex;

mod lnd;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    HttpServer::new(|| {

        let node1_client = LndClient::new(
            dotenvy::var("NODE1_HOST").unwrap(),
            dotenvy::var("NODE1_CERT_PATH").unwrap(),
            dotenvy::var("NODE1_MACAROON_PATH").unwrap()
        );

        let node1 = Node {
            pubkey: dotenvy::var("NODE1_PUBKEY").unwrap(),
            ip: dotenvy::var("NODE1_IP").unwrap(),
            port: 9735,
            network: NodeNetwork::Testnet,
            lightning_impl: NodeLightningImpl::Lnd,
            client: cluster::NodeClient::Lnd(node1_client),
        };

        let nodes = vec![node1];
        let cluster = Cluster::new(nodes);
        App::new().app_data(Data::new(cluster))
            .service(lookup_invoice_handler)
            .service(add_invoice_handler)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}