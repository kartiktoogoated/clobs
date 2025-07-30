use actix_web::{App, HttpServer};

use crate::routes::{create_order, delete_order, get_depth};

pub mod routes;
pub mod inputs;
pub mod outputs;
pub mod orderbook;

#[actix_web::main]
async fn main() -> Result<(), std::io::Error> {
    HttpServer::new(move || {
        App::new()
            .service(create_order)
            .service(delete_order)
            .service(get_depth)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

