use anyhow::Context;
use graphql_client::reqwest::post_graphql_blocking as post_graphql;
use graphql_client::GraphQLQuery;
use log::*;
use reqwest::blocking::Client;
use reqwest::header::HeaderValue;

// The paths are relative to the directory where your `Cargo.toml` is located.
// Both json and the GraphQL schema languaguse itertools::Itertools;e are supported as sources for the schema
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "schemas/kivra.json",
    query_path = "queries/receipts_by_sender.graphql",
    response_derives = "Debug, Serialize"
)]
pub struct ReceiptsBySender;

fn fetch() -> Result<(), anyhow::Error> {
    env_logger::init();

    let kivra_api_token =
        std::env::var("KIVRA_API_TOKEN").expect("Missing KIVRA_API_TOKEN env var");

    let kivra_actor_key =
        std::env::var("KIVRA_ACTOR_KEY").expect("Missing KIVRA_ACTOR_KEY env var");

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", kivra_api_token))?,
    );
    headers.insert("x-actor-key", HeaderValue::from_str(&kivra_actor_key)?);
    headers.insert("x-actor-type", HeaderValue::from_static("user"));
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(
            "Mozilla/5.0 (X11; Linux x86_64; rv:129.0) Gecko/20100101 Firefox/129.0",
        ),
    );

    info!("KIVRA_API");
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:129.0) Gecko/20100101 Firefox/129.0")
        .default_headers(headers)
        .build()?;
    let sender = "";

    let variables = receipts_by_sender::Variables {
        sender_key: sender.to_string(),
        limit: Some(200),
        offset: Some(0),
    };

    let response_body =
        post_graphql::<ReceiptsBySender, _>(&client, "https://bff.kivra.com/graphql", variables)
            .expect("Failed to execute query");

    info!("{:?}", response_body);

    let response_data: receipts_by_sender::ResponseData =
        response_body.data.expect("missing response data");

    // write resonse to file
    std::fs::write(
        "response.json",
        serde_json::to_string_pretty(&response_data).unwrap(),
    )
    .unwrap();

    // read receipts from file
    let response_data: receipts_by_sender::ResponseData =
        serde_json::from_str(&std::fs::read_to_string("response.json").unwrap()).unwrap();

    //info!("{:?}", response_data);
    for receipt in response_data.receipts_v2.unwrap().list {
        info!(
            "{:?} {:?} {:?}",
            receipt.store.name, receipt.purchase_date, receipt.total_amount.amount
        );
    }
    Ok(())
}

type Receipt = receipts_by_sender::baseDetailsFields;
type ReceiptGroup  = std::collections::HashMap<std::string::String, Vec<Receipt>>;

fn grouped() -> Result<ReceiptGroup, anyhow::Error> {
    let response_data: receipts_by_sender::ResponseData =
        serde_json::from_str(&std::fs::read_to_string("response.json")?)?;

    let receipts_by_sender: Vec<Receipt> = response_data.receipts_v2.context("no receipts")?.list;

    // group by purchase date
    let mut grouped_receipts: std::collections::HashMap<
        String, 
        Vec<Receipt>,
    > = std::collections::HashMap::new();

    for receipt in receipts_by_sender {
        let date = receipt.purchase_date.to_string();
        let date = chrono::NaiveDateTime::parse_from_str(&date, "%Y-%m-%dT%H:%M:%SZ")?;

        let key = date.format("%Y-%m").to_string();
        let value = receipt;
        grouped_receipts
            .entry(key)
            .or_insert(Vec::new())
            .push(value);
    }
    Ok(grouped_receipts)
}


fn parse() -> Result<(), anyhow::Error> {
    let grouped_receipts = grouped()?;
    let mut keys: Vec<_> = grouped_receipts.keys().collect();
    keys.sort();
    for key in keys {
        let chunk = grouped_receipts.get::<String>(key).unwrap();
        println!("{}: ", key);
        for receipt in chunk {
            println!(
                "{} {:?} ",
                receipt.store.name, receipt.total_amount.amount
            );
        }
        println!("Total: {}", chunk.iter().map(|x| x.total_amount.amount).sum::<f64>());
        println!()
    }

    Ok(())
}


async fn page() -> Result<String, anyhow::Error> {
    let grouped_receipts = grouped()?;
    let mut keys: Vec<_> = grouped_receipts.keys().collect();
    keys.sort();
    let mut html = String::new();
    for key in keys {
        let chunk = grouped_receipts.get::<String>(key).unwrap();
        html.push_str(&format!("<h2>{}</h2>", key));
        html.push_str("<ul>");
        for receipt in chunk {
            html.push_str(&format!(
                "<li>{} {:?}</li>",
                receipt.store.name, receipt.total_amount.amount
            ));
        }
        html.push_str(&format!("<li>Total: {}</li>", chunk.iter().map(|x| x.total_amount.amount).sum::<f64>()));
        html.push_str("</ul>");
    }

    Ok(format!("<html><meta charset=\"UTF-8\"><body>{}</body></html>", html))

}

#[actix_web::main] 
async fn serve() -> Result<(), anyhow::Error> {

// start a web server that renders a page with the receipts
    use actix_web::{web, App, HttpServer, HttpResponse};


    HttpServer::new(move || {
        App::new().route("/", web::get().to( move || {
            async {
                HttpResponse::Ok()
                    .content_type("text/html")
                    .body(page().await.unwrap())
        }
        }))
    })
    .bind("0.0.0.0:8080")?.run().await?;
    Ok(())
}



fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("No action provided");
        return;
    }
    let action = &args[1];

    match action.as_str() {
        "fetch" => fetch().unwrap(),
        "parse" => parse().unwrap(),
        "serve" => serve().unwrap(),

        _ => println!("No action provided"),
    }
}
