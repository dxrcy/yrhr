use std::collections::{HashMap, HashSet};
use std::fs;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use reqwest::Client;
use scraper::{Html, Selector};

const USER_AGENT: &str = "curl/8.17.0";
const TARGET_CATEGORY: &str = "Hard waste, bundled branches and metals";

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::builder().user_agent(USER_AGENT).build()?;

    print_section("FINDING: REGIONS");
    let regions = get_regions(&client)
        .await
        .with_context(|| "getting regions")?;

    print_section("FINDING: ADDRESSES");
    let searches = get_region_address_searches(&client, &regions)
        .await
        .with_context(|| "getting region address searches")?;

    print_section("FINDING: DATES");
    let results = get_address_dates(&client, &searches)
        .await
        .with_context(|| "getting address dates")?;

    print_section("RESULTS: ADDRESSES");
    for (address, date) in &results {
        println!(
            "{}\t({}, {})\t{}",
            date, address.lat, address.lon, address.line
        );
    }

    print_section("RESULTS: DATES");
    let mut results_unique = results.clone();
    remove_duplicate_dates(&mut results_unique);
    for (address, date) in &results_unique {
        println!("{}\t{}", date, address.line);
    }

    print_section("RESULTS: MAP");
    create_visualization(&results).with_context(|| "creating visualization")?;

    print_hr();
    println!();
    println!();

    Ok(())
}

fn create_visualization(results: &[(Address, NaiveDate)]) -> Result<()> {
    use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};

    const AVAILABLE_COLORS: &[&str] = &[
        "#ff0000", "#0000ff", "#ff00ff", "#880000", "#000088", "#880088", "#008888", "#008800",
        "#000000",
    ];

    let mut features = Vec::new();
    let mut colors = HashMap::<NaiveDate, String>::new();

    for (address, date) in results {
        let len = colors.len();
        let color: &str = &colors
            .entry(*date)
            .or_insert_with(|| AVAILABLE_COLORS[len].to_string());

        let date_str = date.format("%Y-%m-%d").to_string();

        let properties: [(String, serde_json::Value); _] = [
            ("color".to_string(), color.into()),
            ("label".to_string(), date_str.into()),
        ];

        features.push(Feature {
            geometry: Some(Geometry::new(Value::Point(vec![address.lon, address.lat]))),
            properties: Some(properties.into_iter().collect()),
            ..Default::default()
        });
    }

    let geo = GeoJson::from(FeatureCollection {
        features,
        ..Default::default()
    });

    fs::write("viz/points.geojson", geo.to_string()).with_context(|| "writing geojson file")?;

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

        for element in fragment.select(&selector).take(5) {
            let label: String = element.text().collect();
            searches.push(format!("{} {}", label, name).to_lowercase());
        }
    }

    Ok(searches)
}

async fn get_address_dates(
    client: &Client,
    searches: &[String],
) -> Result<Vec<(Address, NaiveDate)>> {
    let mut results = Vec::new();

    for (i, search) in searches.iter().enumerate() {
        if i > 0 {
            println!();
        }

        print_label("search");
        println!("{}", search);

        let Some(address) = get_address_id(&client, search)
            .await
            .with_context(|| "getting address id")?
        else {
            print_label("FAILED");
            println!("address not found");
            continue;
        };

        print_label("address");
        println!("{}", address.line);
        print_label("id");
        println!("{}", address.id);

        let Some(date) = get_pickup_date(&client, &address.id)
            .await
            .with_context(|| "getting pickup date")?
        else {
            print_label("FAILED");
            println!("pickup not available or not found");
            continue;
        };

        print_label("date");
        println!("{}", date);

        results.push((address, date));
    }

    Ok(results)
}

#[derive(Clone, Debug)]
struct Address {
    id: String,
    line: String,
    lat: f64,
    lon: f64,
}

fn remove_duplicate_dates(results: &mut Vec<(Address, NaiveDate)>) {
    let mut dates = HashSet::<NaiveDate>::new();
    results.retain(|(_, date)| dates.insert(*date));
    results.sort_by_key(|(_, date)| *date);
}

async fn get_address_id(client: &Client, search: &str) -> Result<Option<Address>> {
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

    let json: serde_json::Value =
        serde_json::from_str(&text).with_context(|| "parsing response json")?;
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
    let line = data
        .get("AddressSingleLine")
        .with_context(|| "missing json key `AddressSingleLine`")?
        .as_str()
        .with_context(|| "invalid json type for value of key `AddressSingleLine`")?
        .to_string();

    let lat_lon = data
        .get("LatLon")
        .with_context(|| "missing json key `LatLon`")?
        .as_array()
        .with_context(|| "invalid json type for value of key `LatLon`")?;
    let lat = lat_lon
        .get(0)
        .with_context(|| "missing json item in value of key `LatLon`")?
        .as_f64()
        .with_context(|| "invalid json type for value of key `LatLon`")?;
    let lon = lat_lon
        .get(1)
        .with_context(|| "missing json item in value of key `LatLon`")?
        .as_f64()
        .with_context(|| "invalid json type for value of key `LatLon`")?;

    Ok(Some(Address { id, line, lat, lon }))
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

    let json: serde_json::Value =
        serde_json::from_str(&text).with_context(|| "parsing response json")?;
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
