use actix_web::{body::BoxBody, get, post, http::header::ContentType, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::{cluster::{Cluster, ClusterAddInvoice}};

#[derive(Serialize, Deserialize)]
pub struct StatusResponse {
    status: String,
}

impl Responder for StatusResponse {
    type Body = BoxBody;

    fn respond_to(self, _req: &actix_web::HttpRequest) -> actix_web::HttpResponse {
        HttpResponse::Ok()
            .content_type(ContentType::json())
            .body(serde_json::to_string(&self).unwrap())
    }
}

#[get("/status")]
pub async fn status_handler() -> impl Responder {
    StatusResponse {
        status: String::from("ok"),
    }
}

#[derive(Deserialize)]
struct PathParams {
    r_hash: String,
    pubkey: String,
}

#[derive(Serialize)]
struct DataResponse<T> {
    data: T, // Change this to the appropriate response type
}

// Response for errors
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[post("/invoice")]
pub async fn add_invoice_handler(cluster: web::Data<Cluster>, req: web::Json<ClusterAddInvoice>) -> impl Responder {
    let pubkey = req.pubkey.clone();
    let invoice = cluster.add_invoice(req.into_inner(), pubkey).await;  

    match invoice {
        Ok(invoice) => { 
            HttpResponse::Ok()
            .content_type(ContentType::json())
            .json(DataResponse { data: invoice })
        },
        Err(error) => HttpResponse::InternalServerError()
            .content_type(ContentType::json())
            .json(ErrorResponse { error: error.to_string() })
    }
}

#[get("/invoice/{r_hash}/{pubkey}")]
pub async fn lookup_invoice_handler(cluster: web::Data<Cluster>, path: web::Path<(String, String)>) -> impl Responder {
    let (r_hash, pubkey) = path.into_inner();

    let invoice = cluster.lookup_invoice(&r_hash, Some(pubkey)).await;

    match invoice {
        Ok(invoice) => { 
            HttpResponse::Ok()
            .content_type(ContentType::json())
            .json(DataResponse { data: invoice })
        },
        Err(error) => HttpResponse::InternalServerError()
            .content_type(ContentType::json())
            .json(ErrorResponse { error: error.to_string() })
    }
}
