use anyhow::Context;
use graphql_client::reqwest::post_graphql_blocking as post_graphql;
use graphql_client::GraphQLQuery;
use log::*;
use receipt_details::{
    productFields, ProductFieldsQuantityCost, ReceiptDetailsReceiptV2,
    ReceiptDetailsReceiptV2ContentItemsAllItemsItemsOnProductListItem,
};
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
use anyhow::Result;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "schemas/kivra.json",
    query_path = "queries/receipt_details.graphql",
    response_derives = "Debug, Serialize"
)]
pub struct ReceiptDetails;

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

    let response_data: receipts_by_sender::ResponseData =
        response_body.data.expect("missing response data");

    // write resonse to file
    std::fs::write(
        "./cache/receipts.json",
        serde_json::to_string_pretty(&response_data)?,
    )?;
    info!("Wrote response.json");

    // read receipts from file
    let response_data: receipts_by_sender::ResponseData =
        serde_json::from_str(&std::fs::read_to_string("cache/receipts.json")?)?;

    //info!("{:?}", response_data);
    for receipt in response_data.receipts_v2.unwrap().list {
        info!("Fetching receipt {:?}", &receipt.key);

        let variables = receipt_details::Variables {
            key: receipt.key.clone(),
        };

        let took = std::time::Instant::now();
        let receipt_details =
            post_graphql::<ReceiptDetails, _>(&client, "https://bff.kivra.com/graphql", variables)
                .expect("Failed to execute query");
        let took = took.elapsed();
        info!("Took: {:?}", took);
        let filename = format!("cache/receipt-{}.json", &receipt.key);
        info!("Writing {:?}", filename);
        std::fs::write(filename, serde_json::to_string_pretty(&receipt_details)?)?;
    }
    Ok(())
}

type Receipt = receipts_by_sender::baseDetailsFields;
type ReceiptGroup = Vec<(std::string::String, Vec<Receipt>)>;

fn grouped() -> Result<ReceiptGroup, anyhow::Error> {
    let response_data: receipts_by_sender::ResponseData =
        serde_json::from_str(&std::fs::read_to_string("cache/receipts.json")?)?;

    let receipts_by_sender: Vec<Receipt> = response_data.receipts_v2.context("no receipts")?.list;

    let mut grouped_receipts: std::collections::HashMap<String, Vec<Receipt>> =
        std::collections::HashMap::new();
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
    let mut grouped_receipts: ReceiptGroup = grouped_receipts.into_iter().collect();
    grouped_receipts.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(grouped_receipts)
}

fn parse() -> Result<(), anyhow::Error> {
    let grouped_receipts = grouped()?;
    for (key, receipts) in grouped_receipts {
        println!("{}: ", key);
        for receipt in &receipts {
            println!(
                "{} {:?} ",
                &receipt.store.name, &receipt.total_amount.amount
            );
        }
        println!(
            "Total: {}",
            receipts.iter().map(|x| x.total_amount.amount).sum::<f64>()
        );
        println!()
    }

    Ok(())
}

async fn page() -> Result<String, anyhow::Error> {
    let grouped_receipts = grouped()?;

    let mut html = String::new();
    for (key, receipts) in grouped_receipts {
        html.push_str(&format!("<h2>{}</h2>", key));
        html.push_str("<ul>");
        for receipt in &receipts {
            html.push_str(&format!(
                "<li>{} {:?} <a href=\"/receipt/{}\">link</a></li>",
                receipt.store.name, receipt.total_amount.amount, receipt.key
            ));
        }
        html.push_str(&format!(
            "<li>Total: {}</li>",
            receipts.iter().map(|x| x.total_amount.amount).sum::<f64>()
        ));
        html.push_str("</ul>");
    }

    Ok(format!(
        "<html><meta charset=\"UTF-8\"><body>{}</body></html>",
        html
    ))
}
fn receipt(key: String) -> Result<ReceiptDetailsReceiptV2> {
    println!("Fetching receipt {:?}", key);
    let receipt_data: receipt_details::ResponseData = serde_json::from_str(
        &std::fs::read_to_string(format!("./cache/receipt-{}.json", key))?,
    )?;
    receipt_data.receipt_v2.context("no receipt")
}
use actix_web::{get, web, App, HttpServer};

#[actix_web::get("/receipt/{key}")]
async fn receipt_page(key: actix_web::web::Path<String>) -> actix_web::Result<String> {
    let receipt = receipt(key.into_inner()).unwrap();
    let mut body = String::new();
    for item in receipt.content.items.all_items.items {
        println!("{:?}", item)
    }

    Ok(format!(
        "<html><meta charset=\"UTF-8\"><body>{}</body></html>",
        body
    ))
}

#[actix_web::main]
async fn serve() -> Result<()> {
    // start a web server that renders a page with the receipts
    use actix_web::{web, App, HttpResponse, HttpServer};

    let server = HttpServer::new(move || {
        App::new()
            .route(
                "/",
                web::get().to(move || async {
                    HttpResponse::Ok()
                        .content_type("text/html")
                        .body(page().await.unwrap())
                }),
            )
            .service(receipt_page)
            .route(
                "/chart",
                web::get().to(move || async {
                    let chart = all_time_chart().unwrap();
                    let renderer = HtmlRenderer::new("All time", 2000, 400);
                    let out = renderer.render(&chart).unwrap();
                    HttpResponse::Ok().content_type("text/html").body(out)
                }),
            )
    })
    .bind("0.0.0.0:8080")?
    .run();
    println!("Server running on http://localhost:8080/");
    server.await.map_err(anyhow::Error::from)
}

use charming::{
    component::Axis,
    element::{AxisType, Tooltip, Trigger},
    series::Bar,
    theme::Theme,
    Chart, HtmlRenderer,
};

fn all_time_chart() -> Result<Chart, anyhow::Error> {
    let grouped = grouped()?;

    // total amount per month
    let mut data: Vec<(f64, &str)> = Vec::new();
    for (key, value) in grouped.iter() {
        data.push((
            value.iter().map(|x| x.total_amount.amount).sum::<f64>(),
            key,
        ));
    }

    Ok(Chart::new()
        .x_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(data.clone().iter().map(|x| x.1).collect()),
        )
        .y_axis(Axis::new().type_(AxisType::Value))
        .series(Bar::new().data(data.iter().map(|x| x.0).collect()))
        .tooltip(Tooltip::new().trigger(Trigger::Axis)))
}

fn chart() -> anyhow::Result<()> {
    let chart = all_time_chart()?;
    let renderer = HtmlRenderer::new("Nightingale Chart", 2000, 500);
    renderer
        .theme(Theme::Dark)
        .save(&chart, "graph.html")
        .map_err(anyhow::Error::from)
}

fn help() -> anyhow::Result<()> {
    println!("Available actions: fetch, parse, serve, chart");
    Ok(())
}

fn main() -> Result<()> {
    let action = std::env::args().nth(1).unwrap_or("".into());
    match action.as_str() {
        "fetch" => fetch(),
        "parse" => parse(),
        "serve" => serve(),
        "chart" => chart(),
        _ => help(),
    }
}
