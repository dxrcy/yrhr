use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value;

const USER_AGENT: &str = "curl/8.17.0";
const TARGET_CATEGORY: &str = "Hard waste, bundled branches and metals";

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::builder().user_agent(USER_AGENT).build()?;

    print_section("FINDING REGIONS");
    let regions = get_regions(&client)
        .await
        .with_context(|| "getting regions")?;

    print_section("FINDING ADDRESSES");
    let searches = get_region_address_searches(&client, &regions)
        .await
        .with_context(|| "getting region address searches")?;

    print_section("FINDING DATES");
    let results = get_address_dates(&client, &searches)
        .await
        .with_context(|| "getting address dates")?;

    print_section("RESULTS");
    for (address, date) in results {
        println!("{}\t{}", date, address);
    }

    print_hr();
    println!();
    println!();

    Ok(())
}

fn print_section(section: &str) {
    println!();
    print_hr();
    println!(" {}", section);
}
fn print_hr() {
    print!("*****************");
}
fn print_label(label: &str) {
    print!("\t{:10}: ", label);
}

async fn get_regions(client: &Client) -> Result<Vec<(String, String)>> {
    let mut regions = Vec::new();

    let url = "https://australia-streets.openalfa.com/shire-of-yarra-ranges";
    let base_url = "https://australia-streets.openalfa.com";

    let res = client
        .get(url)
        .send()
        .await
        .with_context(|| "sending request")?;
    if !res.status().is_success() {
        bail!("response status not ok: {}", res.status());
    }
    let text = res.text().await.with_context(|| "reading response text")?;

    let fragment = Html::parse_document(&text);
    let selector = Selector::parse(".columns > ul > li > a").expect("parsing const html selector");

    for element in fragment.select(&selector) {
        let name: String = element.text().collect();
        let href = element
            .attr("href")
            .with_context(|| "missing `href` tag on `<a>`")?;
        regions.push((name, base_url.to_string() + href));
    }

    Ok(regions)
}

async fn get_region_address_searches(
    client: &Client,
    regions: &[(String, String)],
) -> Result<Vec<String>> {
    let mut searches = Vec::new();

    for (name, url) in regions {
        print_label("region");
        println!("{}", name);

        let res = client
            .get(url)
            .send()
            .await
            .with_context(|| "sending request")?;
        if !res.status().is_success() {
            bail!("response status not ok: {}", res.status());
        }
        let text = res.text().await.with_context(|| "reading response text")?;

        let fragment = Html::parse_document(&text);
        let selector = Selector::parse(".street-columns > ul > li > label")
            .expect("parsing const html selector");

        for element in fragment.select(&selector) {
            let label: String = element.text().collect();
            searches.push(format!("{} {}", label, name).to_lowercase());
        }
    }

    Ok(searches)
}

async fn get_address_dates(
    client: &Client,
    searches: &[String],
) -> Result<Vec<(String, NaiveDate)>> {
    let mut results = Vec::<(String, NaiveDate)>::new();

    for (i, search) in searches.iter().enumerate() {
        if i > 0 {
            println!();
        }

        print_label("search");
        println!("{}", search);

        let result = get_address_id(&client, search)
            .await
            .with_context(|| "getting address id")?;
        let Some((id, address)) = result else {
            print_label("FAILED");
            println!("address not found");
            continue;
        };

        print_label("address");
        println!("{}", address);
        print_label("id");
        println!("{}", id);

        let result = get_pickup_date(&client, &id)
            .await
            .with_context(|| "getting pickup date")?;
        let Some(date) = result else {
            print_label("FAILED");
            println!("pickup not available or not found");
            continue;
        };

        print_label("date");
        println!("{}", date);

        results.push((address, date));
    }

    let mut dates = HashSet::<NaiveDate>::new();
    results.retain(|(_, date)| dates.insert(*date));
    results.sort_by_key(|(_, date)| *date);

    Ok(results)
}

async fn get_address_id(client: &Client, search: &str) -> Result<Option<(String, String)>> {
    let url = "https://www.yarraranges.vic.gov.au/api/v1/myarea/search?keywords=".to_string()
        + &search.replace(" ", "%20");

    let res = client
        .get(url)
        .send()
        .await
        .with_context(|| "sending request")?;
    if !res.status().is_success() {
        bail!("response status not ok: {}", res.status());
    }
    let text = res.text().await.with_context(|| "reading response text")?;

    let json: Value = serde_json::from_str(&text).with_context(|| "parsing response json")?;
    let Some(data) = json
        .get("Items")
        .with_context(|| "missing json key `Items`")?
        .get(0)
    else {
        // Address didn't match
        return Ok(None);
    };

    let id = data
        .get("Id")
        .with_context(|| "missing json key `Id`")?
        .as_str()
        .with_context(|| "invalid json type for value of key `Id`")?
        .to_string();
    let address = data
        .get("AddressSingleLine")
        .with_context(|| "missing json key `AddressSingleLine`")?
        .as_str()
        .with_context(|| "invalid json type for value of key `AddressSingleLine`")?
        .to_string();

    Ok(Some((id, address)))
}

async fn get_pickup_date(client: &Client, id: &str) -> Result<Option<NaiveDate>> {
    let url = "https://www.yarraranges.vic.gov.au/ocapi/Public/myarea/wasteservices?ocsvclang=en-AU&pageLink=/Our-services/Waste/Find-your-waste-collection-and-burning-off-dates&geolocationid="
           .to_string() + &id;

    let res = client
        .get(url)
        .send()
        .await
        .with_context(|| "sending request")?;
    if !res.status().is_success() {
        bail!("response status not ok: {}", res.status());
    }
    let text = res.text().await.with_context(|| "reading response text")?;

    let json: Value = serde_json::from_str(&text).with_context(|| "parsing response json")?;
    let content = json.get("responseContent").unwrap().as_str().unwrap();

    find_date_in_content(&content)
}

fn find_date_in_content(content: &str) -> Result<Option<NaiveDate>> {
    let fragment = Html::parse_document(content);
    let selector = Selector::parse("article").unwrap();

    for element in fragment.select(&selector) {
        let selector = Selector::parse("h3").unwrap();
        let header = element.select(&selector).next().unwrap();
        let header_text: String = header.text().collect();

        if header_text != TARGET_CATEGORY {
            continue;
        }

        let selector = Selector::parse(".next-service").unwrap();
        let body = element.select(&selector).next().unwrap();
        let body_text: String = body.text().collect();
        let body_trimmed = body_text.trim();

        if let Some(date) = parse_pickup_date(body_trimmed) {
            return Ok(Some(date));
        }

        match body_trimmed {
            "Not available at this address" => (),
            _ => {
                bail!("unexpected body for pickup date: {}", body_trimmed);
            }
        }

        print_label("body");
        return Ok(None);
    }

    Ok(None)
}

fn parse_pickup_date(string: &str) -> Option<NaiveDate> {
    let formats = ["%d %B %Y", "%a %d/%m/%Y"];
    for format in formats {
        if let Ok(date) = NaiveDate::parse_from_str(string, format) {
            return Some(date);
        }
    }
    None
}
