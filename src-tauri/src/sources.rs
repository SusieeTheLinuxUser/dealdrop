//! Every outbound API call lives here. Each fetcher returns `Result<_, String>`
//! so the poll loop can show *why* a source is empty instead of failing silently.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

pub type R<T> = Result<T, String>;

const UA: &str = "DealDrop/0.8.0 (+https://github.com/SusieeTheLinuxUser/dealdrop)";
const UA_BROWSER: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

// ─── Shapes sent to the UI (camelCase = what renderer/app.js reads) ───────────

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Free {
    pub id: String,
    pub title: String,
    pub source: String,
    pub url: String,
    pub image: Option<String>,
    pub app_id: Option<String>,
    pub end_date: Option<String>,
    /// When an upcoming giveaway turns free (Epic "Coming next" only).
    pub starts: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Deal {
    pub id: String,
    pub title: String,
    pub shop: String,
    pub url: String,
    pub image: Option<String>,
    pub cut: u32,
    pub price: f64,
    pub regular: f64,
    pub lowest: Option<f64>,
    pub app_id: Option<String>,
    /// Steam review score in percent and its label ("Very Positive"); CheapShark deals only.
    pub rating: Option<u32>,
    pub rating_text: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    pub discount: u32,
    #[serde(rename = "final")]
    pub fin: f64,
    pub initial: f64,
    pub formatted: Option<String>,
    /// ISO code when known (Steam/Epic are fetched for the US store)
    pub currency: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct WishItem {
    pub store: String, // steam | gog | epic
    pub id: String,
    pub app_id: Option<String>,
    pub name: String,
    pub image: Option<String>,
    pub url: String,
    pub price_info: Option<Price>,
    /// Cheaper offer elsewhere (only set when strictly cheaper than this store's price)
    pub cheapest: Option<Cheapest>,
    /// CheapShark's all-time low (USD)
    pub lowest: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Cheapest {
    pub store: String,
    pub price: f64,
    pub url: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LiveSale {
    pub key: String, // lunar | spring | summer | halloween | autumn | winter
    pub name: String,
    pub url: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Sales {
    pub steam: Vec<Deal>,
    pub gog: Vec<Deal>,
    pub epic: Vec<Deal>,
}

// ─── HTTP ─────────────────────────────────────────────────────────────────────

pub fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(25))
            .build()
            .expect("http client")
    })
}

async fn send(req: reqwest::RequestBuilder) -> R<reqwest::Response> {
    let res = req.send().await.map_err(|e| e.to_string())?;
    let status = res.status();
    if status.is_success() {
        Ok(res)
    } else {
        Err(format!("HTTP {}", status.as_u16()))
    }
}

pub async fn get_json(url: &str) -> R<Value> {
    send(client().get(url)).await?.json().await.map_err(|e| e.to_string())
}

async fn get_text_browser(url: &str) -> R<String> {
    send(client().get(url).header("User-Agent", UA_BROWSER).header("Accept", "application/json"))
        .await?
        .text()
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_text(url: &str) -> R<String> {
    send(client().get(url)).await?.text().await.map_err(|e| e.to_string())
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).filter(|x| x.is_finite())
}

/// A number or numeric string; 0 for anything else (and for "NaN"/"inf", which `parse` accepts).
fn f(v: &Value) -> f64 {
    num(v).unwrap_or(0.0)
}

/// A price: finite and not negative. `None` when missing or garbled, so a bad row is skipped
/// instead of becoming a $0 one (which would look free and trip price targets).
fn money(v: &Value) -> Option<f64> {
    num(v).filter(|x| *x >= 0.0)
}

/// A string that has some text: an id, title, URL or date that is `""` is as good as missing.
pub fn s(v: &Value) -> Option<String> {
    v.as_str().filter(|x| !x.trim().is_empty()).map(str::to_string)
}

fn unescape(t: &str) -> String {
    static ENTITY: OnceLock<Regex> = OnceLock::new();
    // One pass, so "&amp;lt;" becomes the text "&lt;" and not "<". Unknown entities stay as they are.
    re(r"&(#[xX][0-9a-fA-F]{1,6}|#[0-9]{1,7}|[a-zA-Z]{2,6});", &ENTITY).replace_all(t, |c: &regex::Captures| {
        let e = &c[1];
        let ch = match e.strip_prefix('#') {
            Some(n) => match n.strip_prefix(['x', 'X']) { Some(h) => u32::from_str_radix(h, 16), None => n.parse() }.ok()
                .and_then(char::from_u32).filter(|ch| !ch.is_control()),
            None => match e { "lt" => Some('<'), "gt" => Some('>'), "quot" => Some('"'), "amp" => Some('&'), "apos" => Some('\''),
                "nbsp" => Some(' '), "trade" => Some('™'), "reg" => Some('®'), "copy" => Some('©'), _ => None },
        };
        ch.map_or_else(|| c[0].to_string(), String::from)
    }).into_owned()
}

fn re(pat: &'static str, cell: &'static OnceLock<Regex>) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).expect("regex"))
}

pub fn steam_header(app_id: &str) -> String {
    format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/header.jpg")
}

// ─── Steam: storefront ────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SteamItem {
    pub name: String,
    pub price: Option<Price>,
    pub image: String,
}

/// Name, price and artwork for many appids in ONE request (the endpoint the
/// Steam store itself uses; no key needed). Artwork comes from the asset
/// manifest because newer apps no longer have the legacy /apps/{id}/header.jpg.
pub async fn steam_items(ids: &[String]) -> R<HashMap<String, SteamItem>> {
    let mut out = HashMap::new();
    for chunk in ids.chunks(100) {
        let input = json!({
            "ids": chunk.iter().filter_map(|i| i.parse::<u64>().ok()).map(|a| json!({"appid": a})).collect::<Vec<_>>(),
            "context": {"language": "english", "country_code": "US"},
            "data_request": {"include_basic_info": true, "include_assets": true},
        });
        let url = format!(
            "https://api.steampowered.com/IStoreBrowseService/GetItems/v1?input_json={}",
            urlenc(&input.to_string())
        );
        out.extend(parse_steam_items(&get_json(&url).await?));
    }
    Ok(out)
}

fn parse_steam_items(v: &Value) -> HashMap<String, SteamItem> {
    let mut out = HashMap::new();
    for it in v["response"]["store_items"].as_array().into_iter().flatten() {
        let id = it["appid"].as_u64().or(it["id"].as_u64()).map(|n| n.to_string());
        let (Some(id), Some(name)) = (id, s(&it["name"])) else { continue };
        let bo = &it["best_purchase_option"];
        // No readable final price: no price at all (a $0 here would look free and trip price targets).
        let price = money(&bo["final_price_in_cents"]).map(|cents| {
            let fin = cents / 100.0;
            let discount = bo["discount_pct"].as_u64().unwrap_or(0).min(100) as u32;
            let initial = if discount == 0 { fin } else {
                money(&bo["original_price_in_cents"]).map(|c| c / 100.0).filter(|o| *o >= fin)
                    .unwrap_or(if discount < 100 { fin / (1.0 - discount as f64 / 100.0) } else { fin })
            };
            Price { discount, fin, initial, formatted: s(&bo["formatted_final_price"]), currency: Some("USD".into()) }
        });
        let a = &it["assets"];
        let image = a["asset_url_format"].as_str()
            .zip(a["header"].as_str().or(a["small_capsule"].as_str()))
            .map(|(fmt, file)| format!("https://shared.akamai.steamstatic.com/store_item_assets/{}", fmt.replace("${FILENAME}", file)))
            .unwrap_or_else(|| steam_header(&id));
        out.insert(id, SteamItem { name, price, image });
    }
    out
}

fn urlenc(t: &str) -> String {
    url::form_urlencoded::byte_serialize(t.as_bytes()).collect()
}

async fn featured_categories() -> R<Value> {
    get_json("https://store.steampowered.com/api/featuredcategories/?cc=us&l=en").await
}

fn spotlights(fdc: &Value) -> Vec<Value> {
    fdc.as_object()
        .into_iter()
        .flat_map(|o| o.values())
        .filter(|c| c["id"] == "cat_spotlight")
        .flat_map(|c| c["items"].as_array().cloned().unwrap_or_default())
        .collect()
}

/// The themed sale advertised by one of Steam's spotlight banners, if any.
fn sale_from_banners(banners: Vec<Value>) -> Option<LiveSale> {
    static MAJOR: OnceLock<Regex> = OnceLock::new();
    static MAJOR_URL: OnceLock<Regex> = OnceLock::new();
    static KEY: OnceLock<Regex> = OnceLock::new();
    let major = re(r"(?i)(summer|winter|autumn|fall|spring|lunar|halloween|seasonal).*sale|sale.*(summer|winter|autumn|fall|spring|lunar|halloween|seasonal)", &MAJOR);
    let major_url = re(r"(?i)/sale/[^/]*(summer|winter|autumn|spring|lunar|halloween)", &MAJOR_URL);
    let key_re = re(r"(?i)(summer|winter|autumn|fall|spring|lunar|halloween)", &KEY);

    let b = banners.into_iter().find(|b| {
        major.is_match(b["name"].as_str().unwrap_or("")) || major_url.is_match(b["url"].as_str().unwrap_or(""))
    })?;
    let hay = format!("{} {}", b["name"].as_str().unwrap_or(""), b["url"].as_str().unwrap_or(""));
    let key = key_re.find(&hay)?.as_str().to_lowercase();
    Some(LiveSale {
        key: if key == "fall" { "autumn".into() } else { key },
        name: s(&b["name"]).unwrap_or_else(|| "Steam Sale".into()),
        url: s(&b["url"]),
    })
}

fn steam_sale_from(fdc: &Value) -> (bool, Option<LiveSale>) {
    let heavy = fdc["specials"]["items"].as_array().into_iter().flatten()
        .filter(|i| i["discount_percent"].as_u64().unwrap_or(0) >= 50).count();
    let live = sale_from_banners(spotlights(fdc));
    (live.is_some() || heavy >= 8, live)
}

/// `(sale is on, themed sale)`. `None` when `body` is not a JSON object. A JSON object with
/// the wrong shape is `(false, None)` and does not panic.
#[cfg(test)]
fn read_steam_sale(body: &str) -> Option<(bool, Option<LiveSale>)> {
    let fdc: Value = serde_json::from_str(body).ok()?;
    if !fdc.is_object() { return None; }
    Some(steam_sale_from(&fdc))
}

/// A seasonal sale that is live right now, if the store is advertising one.
/// Returns (generic "sale is on" flag, themed sale if recognised).
pub async fn detect_steam_sale() -> R<(bool, Option<LiveSale>)> {
    let fdc = featured_categories().await?;
    Ok(steam_sale_from(&fdc))
}

pub async fn steam_free_featured() -> R<Vec<Free>> {
    Ok(parse_steam_featured(&get_json("https://store.steampowered.com/api/featured/?cc=us&l=en").await?))
}

fn parse_steam_featured(v: &Value) -> Vec<Free> {
    let items = v["large_capsules"].as_array().into_iter().flatten()
        .chain(v["featured_win"].as_array().into_iter().flatten());
    items
        .filter(|i| i["final_price"] == 0 && i["original_price"].as_u64().unwrap_or(0) > 0)
        .filter_map(|i| {
            let id = i["id"].as_u64()?.to_string();
            Some(Free {
                id: format!("steam-{id}"),
                title: s(&i["name"])?,
                source: "steam".into(),
                url: format!("https://store.steampowered.com/app/{id}"),
                image: s(&i["large_capsule_image"]),
                app_id: Some(id),
                kind: "free".into(),
                ..Default::default()
            })
        })
        .collect()
}

pub async fn steam_free_weekends() -> R<Vec<Free>> {
    let ids = steam_free_weekend_ids(&featured_categories().await?);
    let names = if ids.is_empty() { HashMap::new() } else { steam_items(&ids).await.unwrap_or_default() };
    Ok(ids.into_iter().map(|id| Free {
        id: format!("steam-fw-{id}"),
        title: names.get(&id).map(|n| n.name.clone()).unwrap_or_else(|| "Free weekend game".into()),
        source: "steam".into(),
        url: format!("https://store.steampowered.com/app/{id}"),
        image: names.get(&id).map(|n| n.image.clone()).or_else(|| Some(steam_header(&id))),
        app_id: Some(id),
        kind: "weekend".into(),
        ..Default::default()
    }).collect())
}

/// Appids of "Free Weekend" spotlights, plus legacy coming_soon items at 100% off; sorted, unique.
fn steam_free_weekend_ids(fdc: &Value) -> Vec<String> {
    static APP: OnceLock<Regex> = OnceLock::new();
    let app_re = re(r"/app/(\d+)", &APP);
    let mut ids: Vec<String> = spotlights(fdc).iter()
        .filter(|b| b["name"].as_str().is_some_and(|n| n.to_lowercase().contains("free weekend")))
        .filter_map(|b| app_re.captures(b["url"].as_str()?).map(|c| c[1].to_string()))
        .collect();
    // Legacy shape: coming_soon items at 100% off
    ids.extend(fdc["coming_soon"]["items"].as_array().into_iter().flatten()
        .filter(|i| i["discount_percent"] == 100)
        .filter_map(|i| i["id"].as_u64().map(|n| n.to_string())));
    ids.sort();
    ids.dedup();
    ids
}

/// One `appdetails` body. `Some(true)` when `data.type` is `"dlc"`, `Some(false)`
/// for any other type, `None` when there is no type (`success: false`, bad JSON).
fn appdetails_is_dlc(body: &str) -> Option<bool> {
    let v: Value = serde_json::from_str(body).ok()?;
    let entry = v.as_object()?.values().next()?;
    if entry.get("success").and_then(|s| s.as_bool()) != Some(true) {
        return None;
    }
    let kind = entry.get("data")?.get("type")?.as_str()?;
    Some(kind == "dlc")
}

/// Paid apps at 100% off (`specials=1&maxprice=free`). Rows are HTML inside JSON.
/// Id prefix `steam-ftk-` so a failure restore does not collide with `steam-{appid}`.
pub async fn steam_free_to_keep() -> R<Vec<Free>> {
    let search = get_text("https://store.steampowered.com/search/results/?query&start=0&count=50&specials=1&maxprice=free&infinite=1&cc=us&l=en&json=1").await?;
    let mut out = parse_steam_free_to_keep(&search)?;
    // Search HTML has no app type. Label DLC; a failed or skipped lookup leaves the name as it was.
    // These calls are sequential inside the poll's join, and each may use the full 8s timeout,
    // so the labeling pass stops after 12s total (an unlabeled row is the existing fallback).
    let budget_at = tokio::time::Instant::now() + Duration::from_secs(12);
    let mut lookups = 0;
    for item in &mut out {
        let left = budget_at.saturating_duration_since(tokio::time::Instant::now());
        if lookups >= 12 || left < Duration::from_millis(200) {
            break;
        }
        let Some(app_id) = item.app_id.clone() else { continue };
        lookups += 1;
        let url = format!(
            "https://store.steampowered.com/api/appdetails?appids={app_id}&filters=basic&cc=us&l=en"
        );
        let per = left.min(Duration::from_secs(8));
        let Ok(res) = send(client().get(&url).timeout(per)).await else { continue };
        let Ok(Ok(body)) = tokio::time::timeout(per, res.text()).await else { continue };
        if appdetails_is_dlc(&body) == Some(true) {
            item.title.push_str(" (DLC)");
        }
    }
    Ok(out)
}

fn parse_steam_free_to_keep(body: &str) -> R<Vec<Free>> {
    static ROW: OnceLock<Regex> = OnceLock::new();
    static TITLE: OnceLock<Regex> = OnceLock::new();
    static DISC: OnceLock<Regex> = OnceLock::new();
    static FINAL: OnceLock<Regex> = OnceLock::new();
    static ORIG: OnceLock<Regex> = OnceLock::new();
    static CAPSULE: OnceLock<Regex> = OnceLock::new();
    let row = re(r#"data-ds-appid="(\d+)""#, &ROW);
    let title = re(r#"<span class="title">([^<]*)</span>"#, &TITLE);
    let disc = re(r#"data-discount="(\d+)""#, &DISC);
    let fin = re(r#"data-price-final="(\d+)""#, &FINAL);
    let orig = re(r#"discount_original_price">([^<]+)<"#, &ORIG);
    let capsule = re(r#"<img src="([^"]+)""#, &CAPSULE);

    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let html = v["results_html"].as_str().ok_or("no results_html")?;
    let starts: Vec<_> = row.captures_iter(html).map(|c| (c.get(0).unwrap().start(), c[1].to_string())).collect();
    let mut out = vec![];
    for (n, (at, id)) in starts.iter().enumerate() {
        let end = starts.get(n + 1).map_or(html.len(), |x| x.0);
        let chunk = &html[*at..end];
        let (Some(t), Some(d), Some(p)) = (title.captures(chunk), disc.captures(chunk), fin.captures(chunk)) else { continue };
        // 100% off a listed original price: drops permanent free-to-play, which has no such pair.
        if &d[1] != "100" || &p[1] != "0" || orig.captures(chunk).is_none() { continue; }
        out.push(Free {
            id: format!("steam-ftk-{id}"),
            title: unescape(t[1].trim()),
            source: "steam".into(),
            url: format!("https://store.steampowered.com/app/{id}"),
            image: capsule.captures(chunk).map(|c| c[1].to_string()),
            app_id: Some(id.clone()),
            kind: "free".into(),
            ..Default::default()
        });
    }
    Ok(out)
}

/// Steam's own "specials" search — the same list the store's Specials page
/// shows. The JSON wraps server-rendered HTML rows, so we pick the fields out.
pub async fn steam_sales() -> R<Vec<Deal>> {
    let search = get_text("https://store.steampowered.com/search/results/?query&start=0&count=50&specials=1&infinite=1&cc=us&l=en&json=1").await?;
    let mut out = parse_steam_sales(&search)?;
    // Replace the tiny search capsules with proper header art where we can
    let ids: Vec<String> = out.iter().filter_map(|d| d.app_id.clone()).collect();
    if let Ok(items) = steam_items(&ids).await {
        for d in &mut out {
            if let Some(i) = d.app_id.as_ref().and_then(|a| items.get(a)) { d.image = Some(i.image.clone()); }
        }
    }
    Ok(out)
}

fn parse_steam_sales(body: &str) -> R<Vec<Deal>> {
    static ROW: OnceLock<Regex> = OnceLock::new();
    static TITLE: OnceLock<Regex> = OnceLock::new();
    static DISC: OnceLock<Regex> = OnceLock::new();
    static FINAL: OnceLock<Regex> = OnceLock::new();
    static ORIG: OnceLock<Regex> = OnceLock::new();
    static CAPSULE: OnceLock<Regex> = OnceLock::new();
    let row = re(r#"data-ds-appid="(\d+)""#, &ROW);
    let title = re(r#"<span class="title">([^<]*)</span>"#, &TITLE);
    let disc = re(r#"data-discount="(\d+)""#, &DISC);
    let fin = re(r#"data-price-final="(\d+)""#, &FINAL);
    let orig = re(r#"discount_original_price">\$?([\d.,]+)<"#, &ORIG);
    let capsule = re(r#"<img src="([^"]+)""#, &CAPSULE);

    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let html = v["results_html"].as_str().ok_or("no results_html")?;
    let mut out = vec![];
    // Each result row starts at its anchor tag; chunk on the appid attribute.
    let starts: Vec<_> = row.captures_iter(html).map(|c| (c.get(0).unwrap().start(), c[1].to_string())).collect();
    for (n, (at, id)) in starts.iter().enumerate() {
        let end = starts.get(n + 1).map_or(html.len(), |x| x.0);
        let chunk = &html[*at..end];
        let (Some(t), Some(d), Some(p)) = (title.captures(chunk), disc.captures(chunk), fin.captures(chunk)) else { continue };
        let cut: u32 = d[1].parse().unwrap_or(0);
        if cut == 0 || cut > 100 { continue; }
        let Some(price) = p[1].parse::<f64>().ok().filter(|c| c.is_finite()).map(|c| c / 100.0) else { continue };
        let title = unescape(t[1].trim());
        if title.is_empty() { continue; }
        let regular = orig.captures(chunk)
            .and_then(|o| o[1].replace(',', "").parse().ok())
            .unwrap_or(if cut >= 100 { 0.0 } else { price / (1.0 - cut as f64 / 100.0) }); // 100% off: no base to divide
        out.push(Deal {
            id: format!("steam-{id}"),
            title,
            shop: "Steam".into(),
            url: format!("https://store.steampowered.com/app/{id}"),
            image: capsule.captures(chunk).map(|c| c[1].to_string()),
            cut, price, regular,
            lowest: None,
            app_id: Some(id.clone()),
            rating: None, rating_text: None,
        });
    }
    Ok(out)
}

// ─── Steam: wishlist ──────────────────────────────────────────────────────────

pub async fn steam_wishlist(steam_id: &str) -> R<Vec<WishItem>> {
    let ids = parse_steam_wishlist_ids(&get_json(&format!(
        "https://api.steampowered.com/IWishlistService/GetWishlist/v1?steamid={steam_id}"
    )).await?);
    let info = if ids.is_empty() { HashMap::new() } else { steam_items(&ids).await? };
    Ok(ids.into_iter().map(|id| {
        let it = info.get(&id);
        WishItem {
            store: "steam".into(),
            url: format!("https://store.steampowered.com/app/{id}"),
            image: Some(it.map_or_else(|| steam_header(&id), |i| i.image.clone())),
            name: it.map_or_else(|| format!("App {id}"), |i| i.name.clone()),
            price_info: it.and_then(|i| i.price.clone()),
            app_id: Some(id.clone()),
            cheapest: None,
            lowest: None,
            id,
        }
    }).collect())
}

/// Appids in display order: user-ranked first (by rank), then unranked newest first.
fn parse_steam_wishlist_ids(v: &Value) -> Vec<String> {
    let mut raw: Vec<(String, u64, u64)> = v["response"]["items"].as_array().into_iter().flatten()
        .filter_map(|i| Some((i["appid"].as_u64()?.to_string(), i["priority"].as_u64().unwrap_or(0), i["date_added"].as_u64().unwrap_or(0))))
        .collect();
    // priority 0 means "unranked" — user-ranked entries first, then newest
    raw.sort_by_key(|(_, p, added)| (*p == 0, *p, std::cmp::Reverse(*added)));
    raw.into_iter().map(|r| r.0).collect()
}

/// Distinguishes "wishlist is empty" from "we can't see it".
pub async fn steam_profile_private(steam_id: &str) -> bool {
    get_text(&format!("https://steamcommunity.com/profiles/{steam_id}/?xml=1")).await
        .is_ok_and(|x| profile_is_private(&x))
}

fn profile_is_private(xml: &str) -> bool {
    xml.to_lowercase().contains("<privacystate>private</privacystate>")
}

/// Returns (name, avatar) from the public profile XML (no API key needed).
pub async fn steam_profile(steam_id: &str) -> (String, Option<String>) {
    if let Some(p) = get_text(&format!("https://steamcommunity.com/profiles/{steam_id}/?xml=1")).await.ok().and_then(|x| parse_steam_profile(&x)) {
        return p;
    }
    let tail = steam_id.char_indices().nth_back(3).map(|(i, _)| &steam_id[i..]).unwrap_or(steam_id);
    (format!("Steam user {tail}"), None)
}

/// (name, avatar) from profile XML; `None` without a name. Text may be CDATA-wrapped
/// and a name may contain `]` (clan tags like "[TAG] Bob").
fn parse_steam_profile(xml: &str) -> Option<(String, Option<String>)> {
    static NAME: OnceLock<Regex> = OnceLock::new();
    static AVATAR: OnceLock<Regex> = OnceLock::new();
    let name_re = re(r"(?s)<steamID>(?:<!\[CDATA\[(.*?)\]\]>|([^<]*))", &NAME);
    let avatar_re = re(r"(?s)<avatarMedium>(?:<!\[CDATA\[(.*?)\]\]>|([^<]*))", &AVATAR);
    let text = |r: &Regex| r.captures(xml).and_then(|c| c.get(1).or(c.get(2)))
        .map(|m| m.as_str().trim().to_string()).filter(|t| !t.is_empty());
    Some((text(name_re)?, text(avatar_re)))
}

// ─── Free games: Epic + GamerPower ────────────────────────────────────────────

/// (free now, coming next) from one request.
pub async fn epic_free() -> R<(Vec<Free>, Vec<Free>)> {
    let body = get_text_browser("https://store-site-backend-static.ak.epicgames.com/freeGamesPromotions?locale=en-US&country=US&allowCountries=US").await?;
    Ok((parse_epic_free(&body)?, parse_epic_upcoming(&body)?))
}

fn parse_epic_free(body: &str) -> R<Vec<Free>> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    Ok(v["data"]["Catalog"]["searchStore"]["elements"].as_array().into_iter().flatten()
        .filter_map(|g| {
            let offer = &g["promotions"]["promotionalOffers"][0]["promotionalOffers"][0];
            if offer["discountSetting"]["discountPercentage"] != 0 { return None; }
            Some(Free {
                id: format!("epic-{}", s(&g["id"])?),
                title: s(&g["title"])?,
                source: "epic".into(),
                url: epic_url(g),
                end_date: s(&offer["endDate"]),
                image: epic_thumb(g),
                kind: "free".into(),
                ..Default::default()
            })
        })
        .collect())
}

/// Upcoming offers at 0% (free next); one that is only discounted is skipped.
fn parse_epic_upcoming(body: &str) -> R<Vec<Free>> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    Ok(v["data"]["Catalog"]["searchStore"]["elements"].as_array().into_iter().flatten()
        .filter_map(|g| {
            let offer = g["promotions"]["upcomingPromotionalOffers"].as_array()?.iter()
                .flat_map(|o| o["promotionalOffers"].as_array().into_iter().flatten())
                .find(|o| o["discountSetting"]["discountPercentage"] == 0)?;
            Some(Free {
                id: format!("epic-next-{}", s(&g["id"])?),
                title: s(&g["title"])?,
                source: "epic".into(),
                url: epic_url(g),
                starts: s(&offer["startDate"]),
                image: epic_thumb(g),
                kind: "upcoming".into(),
                ..Default::default()
            })
        })
        .collect())
}

fn epic_thumb(g: &Value) -> Option<String> {
    g["keyImages"].as_array().and_then(|k| k.iter().find(|i| i["type"] == "Thumbnail")).and_then(|i| s(&i["url"]))
}

pub fn epic_url(g: &Value) -> String {
    // Each candidate is checked: Epic sends "" or "[]" in productSlug while a mapping has the real slug.
    let slug = [&g["productSlug"], &g["catalogNs"]["mappings"][0]["pageSlug"], &g["offerMappings"][0]["pageSlug"], &g["urlSlug"]]
        .into_iter().find_map(|v| v.as_str().filter(|x| !x.is_empty() && *x != "[]"));
    match slug {
        Some(sl) => format!("https://store.epicgames.com/p/{}", sl.trim_end_matches("/home")),
        None => "https://store.epicgames.com/free-games".into(),
    }
}

/// "Foo (Epic Games) Giveaway" → "Foo", so it dedupes against the store's own listing.
/// Loot titles also lose "(DLC)" and a trailing "Steam Key Giveaway".
fn gp_title(raw: &str) -> String {
    static NOISE: OnceLock<Regex> = OnceLock::new();
    let noise = re(r"(?i)\s*\((epic games|epic|steam|gog|gog\.com|free game|dlc)[^)]*\)|\s+(steam\s+)?(key\s+)?giveaway$", &NOISE);
    let clean = noise.replace_all(raw, "").trim().to_string();
    if clean.is_empty() { raw.trim().to_string() } else { clean } // all noise: keep the raw text
}

pub async fn gamerpower() -> R<Vec<Free>> {
    let body = get_text("https://www.gamerpower.com/api/giveaways?platform=pc&type=game").await?;
    parse_gamerpower(&body, "gp-", "free")
}

/// Free DLC & in-game items (opt-in `show_loot`): shown on the Free page, never notified or counted.
pub async fn gamerpower_loot() -> R<Vec<Free>> {
    let body = get_text("https://www.gamerpower.com/api/giveaways?platform=pc&type=loot").await?;
    parse_gamerpower(&body, "gp-loot-", "loot")
}

fn parse_gamerpower(body: &str, prefix: &str, kind: &str) -> R<Vec<Free>> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    Ok(v.as_array().into_iter().flatten()
        .filter(|g| g["status"].as_str().is_none_or(|st| st.eq_ignore_ascii_case("active")))
        .filter_map(|g| {
            let plat = g["platforms"].as_str().unwrap_or("").to_lowercase();
            let source = if plat.contains("steam") { "steam" } else if plat.contains("epic") { "epic" }
                else if plat.contains("gog") { "gog" } else { "other" };
            let end = s(&g["end_date"]).filter(|e| e != "N/A");
            let title = gp_title(&s(&g["title"])?);
            if title.is_empty() { return None; }
            Some(Free {
                id: format!("{prefix}{}", g["id"].as_u64()?),
                title,
                source: source.into(),
                app_id: g["steam_appid"].as_u64().filter(|&n| n != 0).map(|n| n.to_string()),
                url: s(&g["open_giveaway_url"]).or(s(&g["open_giveaway"])).unwrap_or_else(|| "#".into()),
                end_date: end,
                image: s(&g["thumbnail"]),
                kind: kind.into(),
                starts: None,
            })
        })
        .collect())
}

// ─── GOG sales ────────────────────────────────────────────────────────────────

pub async fn gog_sales() -> R<Vec<Deal>> {
    let body = get_text("https://catalog.gog.com/v1/catalog?limit=48&order=desc:trending&discounted=eq:true&productType=in:game,pack&page=1&countryCode=US&locale=en-US&currencyCode=USD").await?;
    parse_gog_sales(&body)
}

/// Find a GOG product by exact title when a user opens a CheapShark deal.
pub async fn gog_product_for_title(title: &str) -> R<Option<String>> {
    let mut url = url::Url::parse("https://catalog.gog.com/v1/catalog").unwrap();
    url.query_pairs_mut()
        .append_pair("limit", "48")
        .append_pair("productType", "in:game,pack")
        .append_pair("query", title)
        .append_pair("page", "1")
        .append_pair("countryCode", "US")
        .append_pair("locale", "en-US")
        .append_pair("currencyCode", "USD");
    let v = get_json(url.as_str()).await?;
    Ok(parse_gog_product(&v, title))
}

fn parse_gog_product(v: &Value, title: &str) -> Option<String> {
    let wanted = title.trim();
    v["products"].as_array()?.iter().find_map(|p| {
        if !p["title"].as_str()?.trim().eq_ignore_ascii_case(wanted) { return None; }
        p["id"].as_str().map(str::to_string).or_else(|| p["id"].as_u64().map(|id| id.to_string()))
    })
}

fn parse_gog_sales(body: &str) -> R<Vec<Deal>> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    Ok(v["products"].as_array().into_iter().flatten().filter_map(|p| {
        let price = money(&p["price"]["finalMoney"]["amount"])?;
        let regular = money(&p["price"]["baseMoney"]["amount"])?;
        let cut: u32 = p["price"]["discount"].as_str()?.trim_start_matches('-').trim_end_matches('%').parse().ok()?;
        if cut == 0 || cut > 100 { return None; }
        Some(Deal {
            id: format!("gog-{}", s(&p["id"])?),
            title: s(&p["title"])?,
            shop: "GOG".into(),
            url: s(&p["storeLink"])?,
            image: s(&p["coverHorizontal"]),
            cut, price, regular,
            lowest: None,
            app_id: None,
            rating: None, rating_text: None,
        })
    }).collect())
}

// ─── CheapShark rate-limit cooldown ───────────────────────────────────────────

/// (blocked until, last default backoff), unix seconds. Shared by every CheapShark call,
/// so one HTTP 429 stops them all instead of letting the poll extend the block.
static CS_COOLDOWN: std::sync::Mutex<(u64, u64)> = std::sync::Mutex::new((0, 0));
const CS_BACKOFF: u64 = 15 * 60;
const CS_MAX_WAIT: u64 = 2 * 60 * 60;

fn unix_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// After a 429 at `now`: wait Retry-After seconds when sane (capped at 2 h), else the default
/// backoff (15 min, doubling per repeated 429 up to 2 h). Returns (blocked until, next backoff).
fn cooldown_after(prev_backoff: u64, retry_after: Option<&str>, now: u64) -> (u64, u64) {
    let backoff = if prev_backoff == 0 { CS_BACKOFF } else { (prev_backoff * 2).min(CS_MAX_WAIT) };
    let wait = retry_after.and_then(|h| h.trim().parse::<u64>().ok()).filter(|&s| s > 0).map_or(backoff, |s| s.min(CS_MAX_WAIT));
    (now + wait, backoff)
}

/// Start of the cooldown error, so the poll can tell it apart from other CheapShark failures.
pub const CS_BLOCKED: &str = "CheapShark is rate-limiting requests";

/// The error every CheapShark call returns while blocked; `None` once `until` has passed.
fn blocked_msg(until: u64, now: u64) -> Option<String> {
    (until > now).then(|| format!(
        "{CS_BLOCKED}; trying again in {} min ({:02}:{:02} UTC)",
        (until - now).div_ceil(60), until / 3600 % 24, until / 60 % 60,
    ))
}

pub fn cheapshark_blocked() -> Option<String> {
    blocked_msg(CS_COOLDOWN.lock().unwrap().0, unix_secs())
}

/// Every CheapShark request goes through here. While blocked it fails without touching the network.
async fn cheapshark_get(url: &str) -> R<String> {
    if let Some(e) = cheapshark_blocked() { return Err(e) }
    let res = client().get(url).send().await.map_err(|e| e.without_url().to_string())?;
    let status = res.status();
    let now = unix_secs();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = res.headers().get(reqwest::header::RETRY_AFTER).and_then(|v| v.to_str().ok());
        let mut cd = CS_COOLDOWN.lock().unwrap();
        // Parallel requests that 429 together count as one, not one doubling each
        if cd.0 <= now { *cd = cooldown_after(cd.1, retry_after, now); }
        return Err(blocked_msg(cd.0, now).unwrap_or_else(|| "HTTP 429".into()));
    }
    if !status.is_success() { return Err(format!("HTTP {}", status.as_u16())) }
    {
        let mut cd = CS_COOLDOWN.lock().unwrap();
        if cd.0 <= now { *cd = (0, 0); } // a reply sent before a sibling's 429 must not lift the block
    }
    res.text().await.map_err(|e| e.without_url().to_string())
}

async fn cheapshark_json(url: &str) -> R<Value> {
    serde_json::from_str(&cheapshark_get(url).await?).map_err(|e| e.to_string())
}

// ─── Hot deals: CheapShark ────────────────────────────────────────────────────

async fn cheapshark_stores() -> HashMap<String, String> {
    let Ok(v) = cheapshark_json("https://www.cheapshark.com/api/1.0/stores").await else { return HashMap::new() };
    parse_cheapshark_stores(&v)
}

fn parse_cheapshark_stores(v: &Value) -> HashMap<String, String> {
    v.as_array().into_iter().flatten()
        .filter_map(|x| Some((s(&x["storeID"])?, s(&x["storeName"])?)))
        .collect()
}

/// No key needed: CheapShark aggregates ~30 stores and sorts by savings.
pub async fn cheapshark_deals() -> R<Vec<Deal>> {
    let (stores, body) = futures::join!(
        cheapshark_stores(),
        cheapshark_get("https://www.cheapshark.com/api/1.0/deals?sortBy=Savings&onSale=1&pageSize=60&lowerPrice=1&metacritic=60&steamRating=70")
    );
    let body = body?;
    let mut deals = parse_cheapshark_deals(&body, &stores)?;
    let game_by_deal = cheapshark_deals_by_game(&body);
    let (lowest, _) = cheapshark_games(game_by_deal.values().cloned()).await;
    for deal in &mut deals {
        deal.lowest = game_by_deal.get(&deal.id).and_then(|id| lowest.get(id)).copied();
    }
    Ok(deals)
}

/// gameID → all-time low, and dealID → storeID of each game's current deals; 25 ids per request,
/// failed batches are just missing.
async fn cheapshark_games(ids: impl IntoIterator<Item = String>) -> (HashMap<String, f64>, HashMap<String, String>) {
    let ids: Vec<String> = ids.into_iter().collect::<std::collections::HashSet<_>>().into_iter().collect();
    let results = futures::future::join_all(ids.chunks(25).map(|chunk| async move {
        cheapshark_get(&format!("https://www.cheapshark.com/api/1.0/games?ids={}", chunk.join(","))).await.ok()
    })).await;
    let bodies: Vec<String> = results.into_iter().flatten().collect();
    (bodies.iter().flat_map(|b| parse_cheapshark_lowest(b)).collect(), bodies.iter().flat_map(|b| parse_deal_stores(b)).collect())
}

fn parse_deal_stores(body: &str) -> HashMap<String, String> {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return HashMap::new() };
    v.as_object().into_iter().flatten()
        .flat_map(|(_, game)| game["deals"].as_array().into_iter().flatten())
        .filter_map(|d| Some((s(&d["dealID"])?, s(&d["storeID"])?)))
        .collect()
}

fn cheapshark_deals_by_game(body: &str) -> HashMap<String, String> {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return HashMap::new() };
    v.as_array().into_iter().flatten().filter_map(|d| Some((s(&d["dealID"])?, s(&d["gameID"])?))).collect()
}

fn parse_cheapshark_lowest(body: &str) -> HashMap<String, f64> {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return HashMap::new() };
    v.as_object().into_iter().flatten().filter_map(|(id, game)| {
        money(&game["cheapestPriceEver"]["price"]).map(|price| (id.clone(), price))
    }).collect()
}

fn parse_cheapshark_deals(body: &str, stores: &HashMap<String, String>) -> R<Vec<Deal>> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    Ok(v.as_array().into_iter().flatten().filter_map(|d| {
        let id = s(&d["dealID"])?;
        let (price, regular) = (money(&d["salePrice"])?, money(&d["normalPrice"])?);
        let rating = Some(f(&d["steamRatingPercent"]).round() as u32).filter(|&r| r > 0 && f(&d["steamRatingCount"]) > 0.0);
        Some(Deal {
            url: format!("https://www.cheapshark.com/redirect?dealID={id}"),
            id,
            title: s(&d["title"])?,
            shop: stores.get(s(&d["storeID"])?.as_str()).cloned().unwrap_or_else(|| "store".into()),
            image: s(&d["thumb"]),
            cut: f(&d["savings"]).round().clamp(0.0, 100.0) as u32,
            price, regular,
            lowest: None,
            app_id: s(&d["steamAppID"]).filter(|x| !x.is_empty() && x != "0"),
            rating_text: rating.and(s(&d["steamRatingText"])).filter(|x| !x.is_empty()),
            rating,
        })
    }).collect())
}

/// Fill `cheapest` on wishlist items when another store beats the item's own
/// price (what deals.gg shows: the best current price across stores).
/// CheapShark batches Steam appids; GOG/Epic items are matched by exact title.
/// Err (once, not per batch) when CheapShark rate-limited us, so the poll keeps the last good prices.
/// Ok holds the items CheapShark gave no answer for (failed request, past the lookup cap, unknown store).
pub async fn attach_cheapest(items: &mut [WishItem]) -> R<Vec<usize>> {
    let stores = cheapshark_stores().await;
    let mut found: Vec<GameMatch> = vec![];
    let mut answered = vec![false; items.len()];

    // CheapShark prices are USD, so only USD-priced items are compared (GOG may use the account's currency).
    let usd = |w: &WishItem| w.price_info.as_ref().is_some_and(|p| p.currency.as_deref() == Some("USD"));
    // ponytail: at most 250 Steam games (10 requests), discounted first; huge wishlists got CheapShark 429s.
    // Rotate or cache per app id if users want "cheaper elsewhere" on every item of a 1000+ wishlist.
    let steam_idx = most_discounted(items, |w| w.app_id.is_some() && usd(w), 250);
    for chunk in steam_idx.chunks(25) {
        let ids: Vec<&str> = chunk.iter().filter_map(|&i| items[i].app_id.as_deref()).collect();
        let Ok(v) = cheapshark_json(&format!("https://www.cheapshark.com/api/1.0/games?steamAppID={}", ids.join(","))).await else { continue };
        for &i in chunk { answered[i] = true; }
        found.extend(cheapshark_matches(items, chunk, &v));
    }
    // GOG/Epic: one request per title, so 30 at most, discounted first (only those can get "Lowest ever";
    // the rest can still get "cheaper elsewhere" if the cap leaves room).
    let other_idx = most_discounted(items, |w| w.app_id.is_none() && usd(w), 30);
    let lookups = futures::future::join_all(other_idx.iter().map(|&i| {
        let url = format!("https://www.cheapshark.com/api/1.0/games?title={}&exact=1&limit=1", urlenc(&items[i].name));
        async move { (i, cheapshark_json(&url).await.ok()) }
    })).await;
    for (i, v) in lookups {
        if let Some(v) = v { answered[i] = true; found.extend(cheapshark_matches(items, &[i], &v)); }
    }

    // Lowest-ever only matters (and is only shown) for games on sale
    let games: Vec<(usize, String)> = found.iter()
        .filter(|m| items[m.0].price_info.as_ref().is_some_and(|p| p.discount > 0))
        .filter_map(|m| Some((m.0, m.3.clone()?))).collect();
    // Store/link only for games where somebody else is strictly cheaper. The same batched `games?ids=`
    // lookup lists each game's deals with their store, so no request per item (that burst drew 429s).
    let cheaper: Vec<_> = found.into_iter()
        .filter_map(|(i, price, deal, game)| Some((i, price, deal?, game?)))
        .filter(|(i, price, _, _)| items[*i].price_info.as_ref().is_some_and(|p| *price + 0.005 < p.fin))
        .collect();
    let ids = games.iter().map(|(_, g)| g.clone()).chain(cheaper.iter().map(|c| c.3.clone()));
    let (lowest, deal_stores) = cheapshark_games(ids).await;
    assign_lowest(items, &games, &lowest);
    for (i, price, deal, _) in cheaper {
        let store = deal_stores.get(&deal).and_then(|id| stores.get(id).cloned());
        match store {
            Some(store) => items[i].cheapest = Some(Cheapest { store, price, url: format!("https://www.cheapshark.com/redirect?dealID={deal}") }),
            None => answered[i] = false,
        }
    }
    cheapshark_blocked().map_or(Ok((0..items.len()).filter(|&i| !answered[i]).collect()), Err)
}

/// Indexes of the items `keep` accepts, biggest discount first (list order among equals), at most `cap`.
fn most_discounted(items: &[WishItem], keep: impl Fn(&WishItem) -> bool, cap: usize) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..items.len()).filter(|&i| keep(&items[i])).collect();
    idx.sort_by_key(|&i| std::cmp::Reverse(items[i].price_info.as_ref().map_or(0, |p| p.discount)));
    idx.truncate(cap);
    idx
}

/// (item index, cheapest price, cheapestDealID, gameID)
type GameMatch = (usize, f64, Option<String>, Option<String>);

/// Pairs a CheapShark `games?` response with the items it was asked for:
/// a Steam batch matches by appid, a title lookup (one item) takes its first entry.
fn cheapshark_matches(items: &[WishItem], idx: &[usize], v: &Value) -> Vec<GameMatch> {
    let entries = v.as_array().map_or(&[][..], |a| a.as_slice());
    entries.iter().filter_map(|g| {
        let i = match idx {
            [i] if items[*i].app_id.is_none() => *i,
            _ => *idx.iter().find(|&&i| items[i].app_id.is_some() && items[i].app_id == s(&g["steamAppID"]))?,
        };
        Some((i, money(&g["cheapest"])?, s(&g["cheapestDealID"]), s(&g["gameID"])))
    }).collect()
}

fn assign_lowest(items: &mut [WishItem], games: &[(usize, String)], lowest: &HashMap<String, f64>) {
    for (i, game) in games { items[*i].lowest = lowest.get(game).copied(); }
}

/// Cheapest third-party price for one Steam game (wishlist detail panel).
pub async fn cheapest_for_steam_app(app_id: &str) -> R<Option<Value>> {
    let url = format!("https://www.cheapshark.com/api/1.0/deals?steamAppID={}&pageSize=15", urlenc(app_id));
    let (stores, deals) = futures::join!(cheapshark_stores(), cheapshark_json(&url));
    Ok(parse_cheapest_deal(&deals?, &stores))
}

/// Lowest non-free `salePrice` in a CheapShark `deals?` list, as the detail panel's JSON.
fn parse_cheapest_deal(deals: &Value, stores: &HashMap<String, String>) -> Option<Value> {
    let best = deals.as_array().into_iter().flatten()
        .filter(|d| f(&d["salePrice"]) > 0.0)
        .min_by(|a, b| f(&a["salePrice"]).total_cmp(&f(&b["salePrice"])));
    best.map(|b| {
        let deal_id = s(&b["dealID"]);
        json!({
            "store": stores.get(s(&b["storeID"]).unwrap_or_default().as_str()).cloned().unwrap_or_else(|| "store".into()),
            "price": f(&b["salePrice"]),
            "retailPrice": f(&b["normalPrice"]),
            "savingsPercent": f(&b["savings"]).round(),
            "url": deal_id.map(|d| format!("https://www.cheapshark.com/redirect?dealID={d}")),
        })
    })
}

// Live smoke tests: `cargo test -- --ignored --nocapture` (needs network).
#[cfg(test)]
mod live {
    use super::*;

    fn show<T: std::fmt::Debug>(name: &str, r: R<Vec<T>>) {
        let v = r.unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!v.is_empty(), "{name} returned nothing");
        println!("{name}: {} items, first = {:?}", v.len(), v[0]);
    }

    // May legitimately be empty (no free-to-keep promotion running); only an Err is a failure.
    #[tokio::test] #[ignore] async fn steam_free_to_keep_live() {
        let v = steam_free_to_keep().await.expect("steam free-to-keep");
        println!("steam free-to-keep: {} items, first = {:?}", v.len(), v.first());
    }
    #[tokio::test] #[ignore] async fn steam_sales_live() { show("steam sales", steam_sales().await) }
    #[tokio::test] #[ignore] async fn gog_sales_live() { show("gog sales", gog_sales().await) }
    #[tokio::test] #[ignore] async fn deal_title_lookup_live() {
        let gog = gog_product_for_title("Cyberpunk 2077").await.expect("GOG title lookup");
        let epic = crate::stores::epic_product_for_title("Alan Wake 2").await.expect("Epic title lookup");
        println!("GOG Cyberpunk 2077: {gog:?}");
        println!("Epic Alan Wake 2: {epic:?}");
        assert!(gog.is_some(), "GOG exact title not found");
        assert!(epic.is_some(), "Epic exact title not found");
    }
    #[tokio::test] #[ignore] async fn epic_sales_live() { show("epic sales", crate::stores::epic_sales().await) }
    #[tokio::test] #[ignore] async fn epic_free_live() {
        let (now, next) = epic_free().await.expect("epic free");
        println!("epic coming next: {} items, first = {:?}", next.len(), next.first());
        show("epic free", Ok(now));
    }
    #[tokio::test] #[ignore] async fn gamerpower_live() {
        show("gamerpower loot", gamerpower_loot().await);
        show("gamerpower", gamerpower().await);
    }
    #[tokio::test] #[ignore] async fn cheapshark_live() {
        let deals = cheapshark_deals().await.expect("CheapShark deals");
        assert!(deals.iter().any(|deal| deal.lowest.is_some()), "no deals included a lowest-ever price");
        assert!(deals.iter().any(|deal| deal.rating.is_some() && deal.rating_text.is_some()), "no deals included a Steam rating");
        show("cheapshark", Ok(deals));
    }
    #[tokio::test] #[ignore] async fn wishlist_cheapest_live() {
        // STEAM_ID=7656... cargo test wishlist_cheapest_live -- --ignored --nocapture
        let id = std::env::var("STEAM_ID").expect("set STEAM_ID");
        let mut w = steam_wishlist(&id).await.unwrap();
        attach_cheapest(&mut w).await.unwrap();
        let n = w.iter().filter(|i| i.cheapest.is_some()).count();
        println!("{} items, {} have a cheaper non-Steam offer, {} have a lowest-ever price", w.len(), n, w.iter().filter(|i| i.lowest.is_some()).count());
        for i in w.iter().filter(|i| i.cheapest.is_some()).take(5) {
            println!("{} steam ${:.2} -> {:?}", i.name, i.price_info.as_ref().unwrap().fin, i.cheapest);
        }
    }
    #[tokio::test] #[ignore] async fn wishlist_cheaper_store_live() {
        // Priced far above any real offer, so CheapShark must name a cheaper store from the batched games?ids= lookup
        let price = Some(Price { fin: 999.0, currency: Some("USD".into()), ..Default::default() });
        let mut w = vec![WishItem { app_id: Some("620".into()), name: "Portal 2".into(), price_info: price, ..Default::default() }];
        let unanswered = attach_cheapest(&mut w).await.unwrap();
        println!("{:?}", w[0].cheapest);
        assert!(unanswered.is_empty());
        let c = w[0].cheapest.as_ref().expect("a cheaper store");
        assert!(!c.store.is_empty() && c.url.starts_with("https://www.cheapshark.com/redirect?dealID="));
    }
    #[tokio::test] #[ignore] async fn steam_wishlist_prices_live() {
        let ids = vec!["60".to_string(), "1203620".to_string()];
        let m = steam_items(&ids).await.unwrap();
        assert_eq!(m.len(), 2);
        println!("{:?}", m);
    }
}

#[cfg(test)]
pub(crate) mod unit {
    #[test]
    fn appdetails_is_dlc_parses_type() {
        assert_eq!(super::appdetails_is_dlc(r#"{"10":{"success":true,"data":{"type":"game"}}}"#), Some(false));
        assert_eq!(super::appdetails_is_dlc(r#"{"5007040":{"success":true,"data":{"type":"dlc"}}}"#), Some(true));
        assert_eq!(super::appdetails_is_dlc(r#"{"5007040":{"success":false}}"#), None);
    }

    #[test]
    fn gamerpower_titles_are_cleaned() {
        assert_eq!(super::gp_title("Agent A: A puzzle in disguise (Epic Games) Giveaway"), "Agent A: A puzzle in disguise");
        assert_eq!(super::gp_title("Bad Cheese (Epic Games)"), "Bad Cheese");
        assert_eq!(super::gp_title("Foo (Steam) Giveaway"), "Foo");
        assert_eq!(super::gp_title("Voidtrain - Big Catch (DLC) Giveaway"), "Voidtrain - Big Catch");
        assert_eq!(super::gp_title("RIDE 6 Welcome Bikes Pack Steam Key Giveaway"), "RIDE 6 Welcome Bikes Pack");
    }

    #[test]
    fn gamerpower_titles_keep_their_own_words() {
        assert_eq!(super::gp_title("Café Noir (Epic Games) Giveaway"), "Café Noir");
        assert_eq!(super::gp_title("Foo (Steam) Key Giveaway"), "Foo");
        assert_eq!(super::gp_title("Not A Giveaway Indeed"), "Not A Giveaway Indeed"); // only a trailing giveaway is noise
        assert_eq!(super::gp_title("Plain Title"), "Plain Title");
    }

    #[test]
    fn gamerpower_rows_without_a_title_or_an_id_are_skipped() {
        let body = r#"[{"title":"No Id","status":"Active","platforms":"PC"},
                       {"id":5,"title":"No Platform","status":"Active","platforms":"PC"},
                       {"id":6,"title":"Free Game","status":"Active","platforms":"PC, GOG","end_date":"N/A"}]"#;
        let v = super::parse_gamerpower(body, "gp-", "free").unwrap();
        assert_eq!(v.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["gp-5", "gp-6"]);
        assert_eq!(v[0].source, "other"); // no store in the platform list
        assert_eq!(v[1].source, "gog");
        assert!(v[1].end_date.is_none()); // "N/A" is not a date
    }

    #[test]
    fn html_entities_in_titles_are_decoded() {
        assert_eq!(super::unescape("Hades &amp; Co"), "Hades & Co");
        assert_eq!(super::unescape("&lt;Tag&gt;"), "<Tag>");
        assert_eq!(super::unescape("&quot;quoted&quot; &amp; &#39;single&#39;"), "\"quoted\" & 'single'");
        assert_eq!(super::unescape("Trade &trade; Mark &reg; 2026 &copy;"), "Trade ™ Mark ® 2026 ©");
        assert_eq!(super::unescape("plain"), "plain");
        assert_eq!(super::unescape("&amp;lt;b&amp;gt; &amp;amp;"), "&lt;b&gt; &amp;"); // decoded once, not twice
    }

    #[test]
    fn titles_are_encoded_before_they_go_into_a_url() {
        assert_eq!(super::urlenc("Portal 2"), "Portal+2");
        assert_eq!(super::urlenc("a/b"), "a%2Fb");
        assert_eq!(super::urlenc("a&b=c"), "a%26b%3Dc");
        assert_eq!(super::urlenc("ü"), "%C3%BC");
    }

    #[test]
    fn prices_are_read_from_numbers_or_strings() {
        use serde_json::json;
        assert_eq!(super::f(&json!(5)), 5.0);
        assert_eq!(super::f(&json!("5.50")), 5.5);
        assert_eq!(super::f(&json!("n/a")), 0.0);
        assert_eq!(super::f(&json!(null)), 0.0);
        assert_eq!(super::f(&json!(true)), 0.0);
        assert_eq!(super::s(&json!("x")).as_deref(), Some("x"));
        assert_eq!(super::s(&json!(5)), None);
    }

    fn search_body(html: &str) -> String {
        serde_json::json!({"results_html": html}).to_string()
    }

    #[test]
    fn steam_free_to_keep_parses_search_html() {
        let html = r#"
<a data-ds-appid="1091500"><img src="https://cdn.example/c.jpg"><span class="title">Hades &amp; Co </span>
<div data-price-final="0"><div data-discount="100"><div class="discount_original_price">$9.99</div></div></div></a>
<a data-ds-appid="10"><span class="title">Half Off</span><div data-price-final="500" data-discount="50"><div class="discount_original_price">$10.00</div></div></a>
<a data-ds-appid="20"><span class="title">Free To Play</span><div data-discount="100" data-price-final="0"></div></a>"#;
        let v = super::parse_steam_free_to_keep(&search_body(html)).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "steam-ftk-1091500");
        assert_eq!(v[0].title, "Hades & Co");
        assert_eq!(v[0].app_id.as_deref(), Some("1091500"));
        assert_eq!(v[0].source, "steam");
        assert_eq!(v[0].kind, "free");
        assert_eq!(v[0].url, "https://store.steampowered.com/app/1091500");
        assert_eq!(v[0].image.as_deref(), Some("https://cdn.example/c.jpg"));
        assert!(v[0].end_date.is_none());
        assert!(super::parse_steam_free_to_keep("{}").is_err());
        assert!(super::parse_steam_free_to_keep("not json").is_err());
    }

    #[test]
    fn steam_sales_parses_search_html() {
        let html = r#"
<a data-ds-appid="1091500"><img src="https://cdn.example/c.jpg"><span class="title">Hades &amp; Co</span>
<div data-discount="50" data-price-final="50000"><div class="discount_original_price">$1,000.00</div></div></a>
<a data-ds-appid="2"><span class="title">Fallback</span><div data-discount="25" data-price-final="750"></div></a>
<a data-ds-appid="1"><span class="title">No Cut</span><div data-discount="0" data-price-final="100"></div></a>"#;
        let v = super::parse_steam_sales(&search_body(html)).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].id, "steam-1091500");
        assert_eq!(v[0].title, "Hades & Co");
        assert_eq!(v[0].shop, "Steam");
        assert_eq!(v[0].cut, 50);
        assert_eq!(v[0].price, 500.0);
        assert_eq!(v[0].regular, 1000.0);
        assert_eq!(v[0].app_id.as_deref(), Some("1091500"));
        assert_eq!(v[0].url, "https://store.steampowered.com/app/1091500");
        assert_eq!(v[0].image.as_deref(), Some("https://cdn.example/c.jpg"));
        assert_eq!(v[1].id, "steam-2");
        assert_eq!(v[1].title, "Fallback");
        assert_eq!(v[1].cut, 25);
        assert_eq!(v[1].price, 7.5);
        assert_eq!(v[1].regular, 10.0);
        assert!(v[1].image.is_none());
        assert!(super::parse_steam_sales("{}").is_err());
        assert!(super::parse_steam_sales("not json").is_err());
    }

    #[test]
    fn steam_sales_never_show_a_nan_regular_price() {
        let html = r#"<a data-ds-appid="440"><span class="title">Team Fortress 2</span><div data-discount="100" data-price-final="0"></div></a>"#;
        let v = super::parse_steam_sales(&search_body(html)).unwrap();
        assert_eq!(v[0].regular, 0.0);
    }

    #[test]
    fn gog_sales_parses_catalog() {
        let body = r#"{
            "products": [
                {"id":"1454315831","title":"The Witcher 3","storeLink":"https://www.gog.com/en/game/the_witcher_3_wild_hunt","coverHorizontal":"https://images.gog.com/cover.jpg","price":{"finalMoney":{"amount":"5.00"},"baseMoney":{"amount":"10.00"},"discount":"-50%"}},
                {"id":"999","title":"No Link","price":{"finalMoney":{"amount":"1.00"},"baseMoney":{"amount":"2.00"},"discount":"-50%"}}
            ]
        }"#;
        let v = super::parse_gog_sales(body).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "gog-1454315831");
        assert_eq!(v[0].title, "The Witcher 3");
        assert_eq!(v[0].shop, "GOG");
        assert_eq!(v[0].url, "https://www.gog.com/en/game/the_witcher_3_wild_hunt");
        assert_eq!(v[0].image.as_deref(), Some("https://images.gog.com/cover.jpg"));
        assert_eq!(v[0].cut, 50);
        assert_eq!(v[0].price, 5.0);
        assert_eq!(v[0].regular, 10.0);
        assert!(v[0].app_id.is_none());
        assert!(super::parse_gog_sales("nope").is_err());
        assert!(super::parse_gog_sales("{}").unwrap().is_empty());
    }

    #[test]
    fn gog_lookup_requires_an_exact_trimmed_case_insensitive_title() {
        use serde_json::json;
        let v = json!({"products": [
            {"id":"12", "title":"  The Witcher 3  ", "slug":"witcher-3"},
            {"id":"13", "title":"The Witcher 3: Wild Hunt", "slug":"witcher-3-wild-hunt"}
        ]});
        assert_eq!(super::parse_gog_product(&v, " the witcher 3 "), Some("12".into()));
        assert_eq!(super::parse_gog_product(&v, "Witcher 3"), None);
    }

    #[test]
    fn epic_free_parses_promotions() {
        let body = r#"{
            "data": {"Catalog": {"searchStore": {"elements": [
                {"title":"Control","id":"offer1","productSlug":"control/home",
                 "promotions":{"promotionalOffers":[{"promotionalOffers":[{"endDate":"2026-10-22T15:00:00.000Z","discountSetting":{"discountPercentage":0}}]}]},
                 "keyImages":[{"type":"Thumbnail","url":"https://cdn.example/thumb.jpg"}]},
                {"title":"Not Free","id":"offer2",
                 "promotions":{"promotionalOffers":[{"promotionalOffers":[{"discountSetting":{"discountPercentage":20}}]}]}}
            ]}}}
        }"#;
        let v = super::parse_epic_free(body).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "epic-offer1");
        assert_eq!(v[0].title, "Control");
        assert_eq!(v[0].source, "epic");
        assert_eq!(v[0].kind, "free");
        assert_eq!(v[0].end_date.as_deref(), Some("2026-10-22T15:00:00.000Z"));
        assert_eq!(v[0].url, "https://store.epicgames.com/p/control");
        assert_eq!(v[0].image.as_deref(), Some("https://cdn.example/thumb.jpg"));
        assert!(v[0].app_id.is_none());
        assert!(super::parse_epic_free("nope").is_err());
        assert!(super::parse_epic_free("{}").unwrap().is_empty());
    }

    #[test]
    fn epic_upcoming_keeps_only_free_offers() {
        let body = r#"{
            "data": {"Catalog": {"searchStore": {"elements": [
                {"title":"Control","id":"offer1","productSlug":"control/home",
                 "promotions":{"promotionalOffers":[{"promotionalOffers":[{"startDate":"2026-10-15T15:00:00.000Z","endDate":"2026-10-22T15:00:00.000Z","discountSetting":{"discountType":"PERCENTAGE","discountPercentage":0}}]}],"upcomingPromotionalOffers":[]}},
                {"title":"Hades","id":"offer2","catalogNs":{"mappings":[{"pageSlug":"hades","pageType":"productHome"}]},
                 "keyImages":[{"type":"OfferImageWide","url":"https://cdn.example/wide.jpg"},{"type":"Thumbnail","url":"https://cdn.example/hades.jpg"}],
                 "promotions":{"promotionalOffers":[],"upcomingPromotionalOffers":[{"promotionalOffers":[{"startDate":"2026-10-22T15:00:00.000Z","endDate":"2026-10-29T15:00:00.000Z","discountSetting":{"discountType":"PERCENTAGE","discountPercentage":0}}]}]}},
                {"title":"Half Price","id":"offer3","productSlug":"half-price",
                 "promotions":{"promotionalOffers":[],"upcomingPromotionalOffers":[{"promotionalOffers":[{"startDate":"2026-10-22T15:00:00.000Z","endDate":"2026-10-29T15:00:00.000Z","discountSetting":{"discountType":"PERCENTAGE","discountPercentage":50}}]}]}}
            ]}}}
        }"#;
        let now = super::parse_epic_free(body).unwrap();
        assert_eq!(now.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["epic-offer1"]);
        let next = super::parse_epic_upcoming(body).unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].id, "epic-next-offer2");
        assert_eq!(next[0].title, "Hades");
        assert_eq!(next[0].kind, "upcoming");
        assert_eq!(next[0].starts.as_deref(), Some("2026-10-22T15:00:00.000Z"));
        assert_eq!(next[0].url, "https://store.epicgames.com/p/hades");
        assert_eq!(next[0].image.as_deref(), Some("https://cdn.example/hades.jpg"));
        assert!(super::parse_epic_upcoming("nope").is_err());
    }

    #[test]
    fn gamerpower_parses_giveaways() {
        let body = r#"[
            {"id":3100,"title":"Hades (Steam) Giveaway","status":"Active","platforms":"PC, Steam","end_date":"2026-10-24 23:59:59","steam_appid":1145360,"open_giveaway_url":"https://www.gamerpower.com/open/hades","thumbnail":"https://www.gamerpower.com/thumb.jpg"},
            {"id":3101,"title":"Old (Epic Games)","status":"Ended","platforms":"PC, Epic Games Store","end_date":"N/A"}
        ]"#;
        let v = super::parse_gamerpower(body, "gp-", "free").unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "gp-3100");
        assert_eq!(v[0].title, "Hades");
        assert_eq!(v[0].source, "steam");
        assert_eq!(v[0].app_id.as_deref(), Some("1145360"));
        assert_eq!(v[0].end_date.as_deref(), Some("2026-10-24 23:59:59"));
        assert_eq!(v[0].url, "https://www.gamerpower.com/open/hades");
        assert_eq!(v[0].image.as_deref(), Some("https://www.gamerpower.com/thumb.jpg"));
        assert_eq!(v[0].kind, "free");
        assert!(super::parse_gamerpower("nope", "gp-", "free").is_err());
        assert!(super::parse_gamerpower(r#"{"status":0}"#, "gp-", "free").unwrap().is_empty());
        let loot = r#"[{"id":3818,"title":"Squad Weapon Skins - Recon Team Pack (Steam) Giveaway","type":"DLC","status":"Active","platforms":"PC, Steam","end_date":"N/A","open_giveaway_url":"https://www.gamerpower.com/open/squad"}]"#;
        let v = super::parse_gamerpower(loot, "gp-loot-", "loot").unwrap();
        assert_eq!((v[0].id.as_str(), v[0].title.as_str(), v[0].kind.as_str()), ("gp-loot-3818", "Squad Weapon Skins - Recon Team Pack", "loot"));
        assert_eq!((v[0].source.as_str(), v[0].end_date.as_deref()), ("steam", None));
    }

    #[test]
    fn cheapshark_parses_deals() {
        let body = r#"[
            {"dealID":"AbC","gameID":"10","title":"Hades","storeID":"1","salePrice":"5.00","normalPrice":"20.00","savings":"75.6","steamAppID":"1145360","thumb":"https://cdn.example/t.jpg","steamRatingText":"Overwhelmingly Positive","steamRatingPercent":"98","steamRatingCount":"250000"},
            {"dealID":"XyZ","gameID":"11","title":"Loose","storeID":"9","salePrice":"3.00","normalPrice":"3.00","savings":"0","steamAppID":"0","steamRatingText":null,"steamRatingPercent":"0","steamRatingCount":"0"}
        ]"#;
        let stores = std::collections::HashMap::from([("1".into(), "Steam".into())]);
        let v = super::parse_cheapshark_deals(body, &stores).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].id, "AbC");
        assert_eq!(v[0].title, "Hades");
        assert_eq!(v[0].shop, "Steam");
        assert_eq!(v[0].url, "https://www.cheapshark.com/redirect?dealID=AbC");
        assert_eq!(v[0].cut, 76);
        assert_eq!(v[0].price, 5.0);
        assert_eq!(v[0].regular, 20.0);
        assert_eq!(v[0].lowest, None);
        assert_eq!(v[0].app_id.as_deref(), Some("1145360"));
        assert_eq!(v[0].image.as_deref(), Some("https://cdn.example/t.jpg"));
        assert_eq!(v[1].shop, "store");
        assert_eq!(v[1].app_id, None);
        assert_eq!(v[1].url, "https://www.cheapshark.com/redirect?dealID=XyZ");
        assert_eq!(v[0].rating, Some(98));
        assert_eq!(v[0].rating_text.as_deref(), Some("Overwhelmingly Positive"));
        assert_eq!((v[1].rating, v[1].rating_text.as_deref()), (None, None));
        assert!(super::parse_cheapshark_deals("nope", &stores).is_err());
        assert!(super::parse_cheapshark_deals("{}", &stores).unwrap().is_empty());
        assert_eq!(super::cheapshark_deals_by_game(body).get("AbC").map(String::as_str), Some("10"));
        let history = r#"{"10":{"cheapestPriceEver":{"price":"4.25"}},"11":{"cheapestPriceEver":{"price":"2.50"}}}"#;
        assert_eq!(super::parse_cheapshark_lowest(history).get("10"), Some(&4.25));
    }

    #[test]
    fn wishlist_lowest_matches_game_ids() {
        use super::WishItem;
        use serde_json::json;
        let item = |app: Option<&str>| WishItem { app_id: app.map(str::to_string), ..Default::default() };
        let mut items = vec![item(Some("620")), item(Some("400")), item(None)];
        // Steam batch: matched by appid, in any order; an unknown appid is ignored
        let steam = json!([
            {"gameID": "7", "steamAppID": "400", "cheapest": "1.99", "cheapestDealID": "d400"},
            {"gameID": "36", "steamAppID": "620", "cheapest": "9.99", "cheapestDealID": "d620"},
            {"gameID": "99", "steamAppID": "1", "cheapest": "0.50", "cheapestDealID": "d1"}
        ]);
        let mut found = super::cheapshark_matches(&items, &[0, 1], &steam);
        // Title lookup for the non-Steam item, missing its deal id
        found.extend(super::cheapshark_matches(&items, &[2], &json!([{"gameID": "55", "cheapest": "3.00"}])));
        found.extend(super::cheapshark_matches(&items, &[2], &json!({"error": "nope"})));
        let games: Vec<(usize, String)> = found.iter().filter_map(|m| Some((m.0, m.3.clone()?))).collect();
        assert_eq!(games, vec![(1, "7".into()), (0, "36".into()), (2, "55".into())]);
        assert_eq!(found[1].2.as_deref(), Some("d620"));
        assert_eq!(found[2].2, None);

        let lowest = super::parse_cheapshark_lowest(r#"{"36":{"cheapestPriceEver":{"price":"1.99"}},"7":{"cheapestPriceEver":{}}}"#);
        super::assign_lowest(&mut items, &games, &lowest);
        assert_eq!(items[0].lowest, Some(1.99));
        assert_eq!(items[1].lowest, None);
        assert_eq!(items[2].lowest, None);
    }

    #[test]
    fn wishlist_lookups_take_the_most_discounted_first() {
        use super::{Price, WishItem};
        let item = |epic: bool, discount| WishItem { app_id: (!epic).then(|| "1".into()), price_info: Some(Price { discount, ..Default::default() }), ..Default::default() };
        // a discounted Epic game late in the list still gets one of the capped lookups
        let items = vec![item(true, 0), item(false, 90), item(true, 0), item(true, 50), item(true, 0), item(true, 75)];
        assert_eq!(super::most_discounted(&items, |w| w.app_id.is_none(), 3), vec![5, 3, 0]);
        assert_eq!(super::most_discounted(&items, |w| w.app_id.is_none(), 30), vec![5, 3, 0, 2, 4]);
    }

    #[test]
    fn sale_banners_map_to_theme_keys() {
        use serde_json::json;
        let key = |name: &str, url: &str| super::sale_from_banners(vec![json!({"name": name, "url": url})]).map(|s| s.key);
        assert_eq!(key("Steam Summer Sale", "https://store.steampowered.com/sale/summer2026"), Some("summer".into()));
        assert_eq!(key("Steam Fall Sale", "https://store.steampowered.com/sale/fall"), Some("autumn".into()));
        assert_eq!(key("Lunar New Year Sale", "https://store.steampowered.com/sale/lunarnewyear"), Some("lunar".into()));
        assert_eq!(key("Weekly Deals", "https://store.steampowered.com/sale/weekly"), None);
        assert_eq!(key("Summer Fest", "https://store.steampowered.com/sale/summerfest"), Some("summer".into()));
        assert!(super::sale_from_banners(vec![]).is_none());
    }

    #[test]
    fn steam_sale_page_reads_spotlight_or_nothing() {
        // Trimmed featuredcategories shape: keys "0","1",…, id "cat_spotlight",
        // items {name, header_image, body, url}. specials items carry discount_percent.
        let sale = r#"{
            "0": {"id":"cat_spotlight","name":"Spotlights","items":[
                {"name":"Publisher Sale","header_image":"https://cdn.example/p.jpg","body":"","url":"https://store.steampowered.com/sale/wbpubsale2026"}
            ]},
            "1": {"id":"cat_spotlight","name":"Spotlights","items":[
                {"name":"Autumn Sale","header_image":"https://cdn.example/a.jpg","body":"","url":"https://store.steampowered.com/sale/autumn2026"}
            ]},
            "specials": {"id":"cat_specials","name":"Specials","items":[{"id":1,"discount_percent":10}]}
        }"#;
        let (on, live) = super::read_steam_sale(sale).unwrap();
        assert!(on);
        let live = live.unwrap();
        assert_eq!(live.key, "autumn");
        assert_eq!(live.name, "Autumn Sale");
        assert_eq!(live.url.as_deref(), Some("https://store.steampowered.com/sale/autumn2026"));

        let quiet = r#"{
            "0": {"id":"cat_spotlight","name":"Spotlights","items":[
                {"name":"Free Weekend","header_image":"https://cdn.example/f.jpg","body":"","url":"https://store.steampowered.com/app/313120"},
                {"name":"Publisher Sale","header_image":"https://cdn.example/p.jpg","body":"","url":"https://store.steampowered.com/sale/wbpubsale2026"}
            ]},
            "specials": {"id":"cat_specials","items":[
                {"discount_percent":70},{"discount_percent":75},{"discount_percent":85}
            ]}
        }"#;
        let (on, live) = super::read_steam_sale(quiet).unwrap();
        assert!(!on);
        assert!(live.is_none());

        let heavy = r#"{"specials":{"items":[
            {"discount_percent":50},{"discount_percent":50},{"discount_percent":50},{"discount_percent":50},
            {"discount_percent":50},{"discount_percent":50},{"discount_percent":50},{"discount_percent":50}
        ]}}"#;
        let (on, live) = super::read_steam_sale(heavy).unwrap();
        assert!(on);
        assert!(live.is_none());
        let seven = r#"{"specials":{"items":[
            {"discount_percent":50},{"discount_percent":50},{"discount_percent":50},{"discount_percent":50},
            {"discount_percent":50},{"discount_percent":50},{"discount_percent":50}
        ]}}"#;
        let (on, live) = super::read_steam_sale(seven).unwrap();
        assert!(!on);
        assert!(live.is_none());

        let (on, live) = super::read_steam_sale(r#"{"specials":"nope"}"#).unwrap();
        assert!(!on);
        assert!(live.is_none());
        assert!(super::read_steam_sale("{").is_none());
        assert!(super::read_steam_sale("").is_none());
        assert!(super::read_steam_sale("null").is_none());
        assert!(super::read_steam_sale("[1]").is_none());
    }

    #[test]
    fn steam_items_parses_store_browse() {
        use serde_json::json;
        // IStoreBrowseService/GetItems: string cents, discount_pct, asset manifest
        let v = json!({"response": {"store_items": [
            {"item_type":0,"id":620,"success":1,"name":"Portal 2","appid":620,
             "best_purchase_option":{"final_price_in_cents":"98","original_price_in_cents":"979","formatted_final_price":"$0.98","discount_pct":90},
             "assets":{"asset_url_format":"steam/apps/620/${FILENAME}?t=1745363004","header":"header.jpg","small_capsule":"capsule_231x87.jpg"}},
            {"id":440,"name":"Team Fortress 2","appid":440,"is_free":true,
             "assets":{"asset_url_format":"steam/apps/440/${FILENAME}","small_capsule":"capsule.jpg"}},
            {"id":10,"name":"Counter-Strike","best_purchase_option":{"final_price_in_cents":"999","original_price_in_cents":"1999","discount_pct":0}},
            {"id":999999,"success":2}
        ]}});
        let m = super::parse_steam_items(&v);
        assert_eq!(m.len(), 3);
        let p2 = &m["620"];
        assert_eq!(p2.name, "Portal 2");
        assert_eq!(p2.image, "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/620/header.jpg?t=1745363004");
        let pr = p2.price.as_ref().unwrap();
        assert_eq!((pr.discount, pr.fin, pr.initial), (90, 0.98, 9.79));
        assert_eq!(pr.formatted.as_deref(), Some("$0.98"));
        assert_eq!(pr.currency.as_deref(), Some("USD"));
        // Free-to-play: no purchase option → no price; small_capsule when there's no header
        assert!(m["440"].price.is_none());
        assert_eq!(m["440"].image, "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/440/capsule.jpg");
        // No discount: initial is the final price (original is ignored); no assets → legacy header; `id` when no `appid`
        let cs = m["10"].price.as_ref().unwrap();
        assert_eq!((cs.discount, cs.fin, cs.initial), (0, 9.99, 9.99));
        assert!(cs.formatted.is_none());
        assert_eq!(m["10"].image, super::steam_header("10"));
        assert!(super::parse_steam_items(&json!({"response": {}})).is_empty());
    }

    #[test]
    fn steam_featured_keeps_only_paid_games_at_zero() {
        use serde_json::json;
        let v = json!({
            "large_capsules": [
                {"id":1,"name":"Gift","final_price":0,"original_price":1999,"large_capsule_image":"https://cdn.example/l.jpg"}
            ],
            "featured_win": [
                {"id":2,"name":"Always Free","final_price":0,"original_price":0},
                {"id":3,"name":"Paid","final_price":499,"original_price":999},
                {"id":4,"name":"Also Gift","final_price":0,"original_price":499},
                {"name":"No Id","final_price":0,"original_price":499}
            ]
        });
        let v = super::parse_steam_featured(&v);
        assert_eq!(v.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["steam-1", "steam-4"]);
        assert_eq!(v[0].title, "Gift");
        assert_eq!(v[0].url, "https://store.steampowered.com/app/1");
        assert_eq!(v[0].image.as_deref(), Some("https://cdn.example/l.jpg"));
        assert_eq!(v[0].app_id.as_deref(), Some("1"));
        assert_eq!((v[0].source.as_str(), v[0].kind.as_str()), ("steam", "free"));
        assert!(v[1].image.is_none());
        assert!(super::parse_steam_featured(&json!({})).is_empty());
    }

    #[test]
    fn steam_free_weekend_ids_from_spotlights_and_coming_soon() {
        let fdc: serde_json::Value = serde_json::from_str(r#"{
            "0": {"id":"cat_spotlight","items":[
                {"name":"Free Weekend - Play Now","url":"https://store.steampowered.com/app/313120/?snr=1"},
                {"name":"FREE WEEKEND","url":"https://store.steampowered.com/sale/nolink"},
                {"name":"Autumn Sale","url":"https://store.steampowered.com/app/999"}
            ]},
            "1": {"id":"cat_spotlight","items":[{"name":"Free weekend","url":"https://store.steampowered.com/app/313120"}]},
            "coming_soon": {"items":[{"id":42,"discount_percent":100},{"id":43,"discount_percent":90}]}
        }"#).unwrap();
        assert_eq!(super::steam_free_weekend_ids(&fdc), ["313120", "42"]);
        assert!(super::steam_free_weekend_ids(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn steam_wishlist_orders_ranked_then_newest() {
        use serde_json::json;
        let v = json!({"response": {"items": [
            {"appid": 10, "priority": 0, "date_added": 100},
            {"appid": 20, "priority": 2, "date_added": 50},
            {"appid": 30, "priority": 0, "date_added": 300},
            {"appid": 40, "priority": 1, "date_added": 10},
            {"appid": 50},
            {"priority": 3}
        ]}});
        assert_eq!(super::parse_steam_wishlist_ids(&v), ["40", "20", "30", "10", "50"]);
        // Empty wishlist: Steam returns an empty response object
        assert!(super::parse_steam_wishlist_ids(&json!({"response": {}})).is_empty());
    }

    #[test]
    fn steam_profile_xml_reads_name_avatar_and_privacy() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<profile><steamID64>76561197960287930</steamID64><steamID><![CDATA[[TAG] Rabscuttle ]]></steamID>
<privacyState>public</privacyState>
<avatarIcon><![CDATA[https://avatars.steamstatic.com/abc.jpg]]></avatarIcon>
<avatarMedium><![CDATA[https://avatars.steamstatic.com/abc_medium.jpg]]></avatarMedium></profile>"#;
        let (name, avatar) = super::parse_steam_profile(xml).unwrap();
        assert_eq!(name, "[TAG] Rabscuttle");
        assert_eq!(avatar.as_deref(), Some("https://avatars.steamstatic.com/abc_medium.jpg"));
        assert!(!super::profile_is_private(xml));
        // Plain text (no CDATA), no avatar
        let plain = "<profile><steamID>Bob</steamID><privacyState>Private</privacyState></profile>";
        assert_eq!(super::parse_steam_profile(plain), Some(("Bob".into(), None)));
        assert!(super::profile_is_private(plain));
        // Unknown profile: Steam answers with an <error> document
        assert_eq!(super::parse_steam_profile("<response><error><![CDATA[The specified profile could not be found.]]></error></response>"), None);
        assert_eq!(super::parse_steam_profile("<profile><steamID><![CDATA[ ]]></steamID></profile>"), None);
    }

    #[test]
    fn epic_urls_fall_back_through_slugs() {
        use serde_json::json;
        assert_eq!(super::epic_url(&json!({"productSlug": "[]", "offerMappings": [{"pageSlug": "from-offer"}]})), "https://store.epicgames.com/p/from-offer");
        assert_eq!(super::epic_url(&json!({"productSlug": "", "urlSlug": "url-slug"})), "https://store.epicgames.com/p/url-slug");
        assert_eq!(super::epic_url(&json!({"title": "Mystery Game"})), "https://store.epicgames.com/free-games");
        // Mystery-game giveaways: a free element with no id is skipped, not an error
        let body = r#"{"data":{"Catalog":{"searchStore":{"elements":[
            {"title":"Mystery Game","promotions":{"promotionalOffers":[{"promotionalOffers":[{"discountSetting":{"discountPercentage":0}}]}]}},
            {"title":"No Promo","id":"x","promotions":null}
        ]}}}}"#;
        assert!(super::parse_epic_free(body).unwrap().is_empty());
        assert!(super::parse_epic_upcoming(body).unwrap().is_empty());
    }

    #[test]
    fn cheapshark_stores_and_cheapest_deal() {
        use serde_json::json;
        let stores = super::parse_cheapshark_stores(&json!([
            {"storeID":"1","storeName":"Steam","isActive":1},
            {"storeID":"25","storeName":"Epic Games Store","isActive":1},
            {"storeName":"No Id"}
        ]));
        assert_eq!(stores.len(), 2);
        assert_eq!(stores["25"], "Epic Games Store");
        assert!(super::parse_cheapshark_stores(&json!({"error": "rate limited"})).is_empty());

        let deals = json!([
            {"dealID":"steam1","storeID":"1","salePrice":"9.99","normalPrice":"19.99","savings":"50.025"},
            {"dealID":"free","storeID":"25","salePrice":"0.00","normalPrice":"19.99","savings":"100"},
            {"dealID":"cheap","storeID":"25","salePrice":"7.49","normalPrice":"19.99","savings":"62.531"},
            {"dealID":"odd","storeID":"77","salePrice":"8.00","normalPrice":"19.99","savings":"59.9"}
        ]);
        let best = super::parse_cheapest_deal(&deals, &stores).unwrap();
        assert_eq!(best, json!({
            "store": "Epic Games Store", "price": 7.49, "retailPrice": 19.99, "savingsPercent": 63.0,
            "url": "https://www.cheapshark.com/redirect?dealID=cheap",
        }));
        // Unknown store id → "store"; only free deals or none → no offer
        let only = super::parse_cheapest_deal(&json!([deals[3]]), &stores).unwrap();
        assert_eq!(only["store"], "store");
        assert!(super::parse_cheapest_deal(&json!([deals[1]]), &stores).is_none());
        assert!(super::parse_cheapest_deal(&json!([]), &stores).is_none());
    }

    #[test]
    fn cheapshark_lowest_skips_bad_entries() {
        let body = r#"{"10":{"info":{"title":"Hades"},"cheapestPriceEver":{"price":"0.00","date":1}},"11":{"cheapestPriceEver":{"price":"n/a"}},"12":[]}"#;
        let m = super::parse_cheapshark_lowest(body);
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("10"), Some(&0.0));
        assert!(super::parse_cheapshark_lowest("[]").is_empty());
        assert!(super::parse_cheapshark_lowest("not json").is_empty());
        let games = r#"{"36":{"deals":[{"storeID":"1","dealID":"a%3D"},{"storeID":"7"}]},"7":{"deals":[{"storeID":"25","dealID":"b"}]},"8":[]}"#;
        let m = super::parse_deal_stores(games);
        assert_eq!((m.len(), m.get("a%3D").map(String::as_str), m.get("b").map(String::as_str)), (2, Some("1"), Some("25")));
        assert!(super::parse_deal_stores("not json").is_empty());
        assert!(super::cheapshark_deals_by_game("not json").is_empty());
    }

    #[test]
    fn cheapshark_cooldown_rules() {
        use super::{blocked_msg, cooldown_after};
        let now = 1_000_000;
        // Retry-After honoured, capped at 2 h; the default backoff still advances
        assert_eq!(cooldown_after(0, Some(" 120 "), now), (now + 120, 900));
        assert_eq!(cooldown_after(0, Some("99999"), now), (now + 7200, 900));
        // Missing, garbage, zero or HTTP-date headers fall back to 15 min, doubling up to 2 h
        for h in [None, Some("soon"), Some("0"), Some("-5"), Some("Wed, 21 Oct 2026 07:28:00 GMT")] {
            assert_eq!(cooldown_after(0, h, now), (now + 900, 900), "{h:?}");
        }
        assert_eq!(cooldown_after(900, None, now), (now + 1800, 1800));
        assert_eq!(cooldown_after(3600, None, now), (now + 7200, 7200));
        assert_eq!(cooldown_after(7200, None, now), (now + 7200, 7200));
        // Blocked until the deadline, then not; (0, 0) after a success is never blocked
        // 1_000_900 s = 11 days 14:01:40 UTC
        assert_eq!(blocked_msg(now + 900, now).unwrap(), "CheapShark is rate-limiting requests; trying again in 15 min (14:01 UTC)");
        assert!(blocked_msg(now + 900, now + 899).unwrap().contains("in 1 min"));
        assert_eq!(blocked_msg(now + 900, now + 900), None);
        assert_eq!(blocked_msg(0, now), None);
    }

    #[test]
    fn prices_are_finite_and_never_negative() {
        use serde_json::json;
        for bad in [json!("NaN"), json!("inf"), json!("-inf"), json!("infinity")] {
            assert_eq!(super::f(&bad), 0.0, "{bad}");
            assert_eq!(super::money(&bad), None, "{bad}");
        }
        assert_eq!(super::money(&json!("-1.50")), None);
        assert_eq!(super::money(&json!(-3)), None);
        assert_eq!(super::money(&json!(null)), None);
        assert_eq!(super::money(&json!("1,234.56")), None); // a thousands separator is a parse failure, not $1
        assert_eq!(super::money(&json!(" 4.99 ")), Some(4.99));
        assert_eq!(super::money(&json!(0)), Some(0.0));
        assert_eq!(super::money(&json!("0.00")), Some(0.0));
    }

    #[test]
    fn cheapshark_rows_without_a_real_price_are_skipped() {
        let stores = std::collections::HashMap::new();
        let body = r#"[
            {"dealID":"ok","gameID":"1","title":"Fine","storeID":"1","salePrice":"5.00","normalPrice":"10.00","savings":"50","steamAppID":""},
            {"dealID":"nosale","gameID":"2","title":"No Sale Price","storeID":"1","normalPrice":"10.00","savings":"50"},
            {"dealID":"nan","gameID":"3","title":"NaN","storeID":"1","salePrice":"NaN","normalPrice":"10.00","savings":"50"},
            {"dealID":"neg","gameID":"4","title":"Negative","storeID":"1","salePrice":"-1","normalPrice":"10.00","savings":"50"},
            {"dealID":"nonormal","gameID":"5","title":"No Normal","storeID":"1","salePrice":"5.00","savings":"50"},
            {"dealID":"over","gameID":"6","title":"Over","storeID":"1","salePrice":"1.00","normalPrice":"10.00","savings":"150"},
            {"dealID":"under","gameID":"7","title":"Under","storeID":"1","salePrice":"1.00","normalPrice":"10.00","savings":"-20"}
        ]"#;
        let v = super::parse_cheapshark_deals(body, &stores).unwrap();
        assert_eq!(v.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["ok", "over", "under"]);
        assert_eq!(v[0].app_id, None); // an empty steamAppID is not an app
        assert_eq!((v[1].cut, v[2].cut), (100, 0)); // never above 100% or below 0%
        for d in &v { assert!(d.price.is_finite() && d.price >= 0.0 && d.regular.is_finite()); }
    }

    #[test]
    fn cheapshark_lowest_and_cheapest_ignore_non_prices() {
        use serde_json::json;
        let m = super::parse_cheapshark_lowest(r#"{"1":{"cheapestPriceEver":{"price":"NaN"}},"2":{"cheapestPriceEver":{"price":"-3.00"}},"3":{"cheapestPriceEver":{"price":"inf"}},"4":{"cheapestPriceEver":{"price":"2.00"}}}"#);
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("4"), Some(&2.0));
        // A games?steamAppID= entry without a usable "cheapest" must not become a $0 cheaper offer
        let item = super::WishItem { app_id: Some("620".into()), ..Default::default() };
        let v = json!([{"gameID":"1","steamAppID":"620","cheapestDealID":"d"}, {"gameID":"2","steamAppID":"620","cheapest":"NaN","cheapestDealID":"d"}]);
        assert!(super::cheapshark_matches(&[item], &[0], &v).is_empty());
    }

    #[test]
    fn gog_rows_without_a_real_price_or_cut_are_skipped() {
        let row = |id: &str, fin: &str, base: &str, disc: &str| format!(
            r#"{{"id":"{id}","title":"T{id}","storeLink":"https://www.gog.com/en/game/t{id}","price":{{"finalMoney":{{"amount":"{fin}"}},"baseMoney":{{"amount":"{base}"}},"discount":"{disc}"}}}}"#);
        let body = format!(r#"{{"products":[{},{},{},{},{},{},{},{{"id":"9","title":"No Price","storeLink":"https://www.gog.com/en/game/x","price":{{"discount":"-50%"}}}}]}}"#,
            row("1", "5.00", "10.00", "-50%"), row("2", "NaN", "10.00", "-50%"), row("3", "5.00", "1,000.00", "-50%"),
            row("4", "5.00", "10.00", "-150%"), row("5", "10.00", "10.00", "-0%"), row("6", "-5.00", "10.00", "-50%"), row("7", "0.00", "10.00", "-100%"));
        let v = super::parse_gog_sales(&body).unwrap();
        assert_eq!(v.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["gog-1", "gog-7"]);
        assert_eq!((v[1].price, v[1].cut), (0.0, 100)); // a real 100% off stays
    }

    #[test]
    fn steam_search_rows_with_a_nonsense_cut_or_price_are_skipped() {
        let html = r#"
<a data-ds-appid="1"><span class="title">Over</span><div data-discount="150" data-price-final="500"></div></a>
<a data-ds-appid="2"><span class="title">Huge</span><div data-discount="50" data-price-final="99999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999"></div></a>
<a data-ds-appid="3"><span class="title"></span><div data-discount="50" data-price-final="500"></div></a>
<a data-ds-appid="4"><span class="title">Fine</span><div data-discount="50" data-price-final="500"><div class="discount_original_price">$1,234.56</div></div></a>
<a data-ds-appid="5"><span class="title">Dot</span><div data-discount="20" data-price-final="800"><div class="discount_original_price">$.</div></div></a>"#;
        let v = super::parse_steam_sales(&search_body(html)).unwrap();
        assert_eq!(v.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["steam-4", "steam-5"]);
        assert_eq!(v[0].regular, 1234.56);
        assert_eq!(v[1].regular, 10.0); // unparsable original: derived from the cut
        for d in &v { assert!(d.price.is_finite() && d.regular.is_finite() && d.regular >= d.price); }
    }

    #[test]
    fn steam_items_never_invent_a_zero_price() {
        use serde_json::json;
        let v = json!({"response": {"store_items": [
            {"appid":1,"name":"No Price","best_purchase_option":{"discount_pct":50}},
            {"appid":2,"name":"NaN","best_purchase_option":{"final_price_in_cents":"NaN","discount_pct":0}},
            {"appid":3,"name":"No Original","best_purchase_option":{"final_price_in_cents":"500","discount_pct":50}},
            {"appid":4,"name":"Over","best_purchase_option":{"final_price_in_cents":"500","original_price_in_cents":"1000","discount_pct":400}},
            {"appid":5,"name":"Free Sale","best_purchase_option":{"final_price_in_cents":"0","discount_pct":100}}
        ]}});
        let m = super::parse_steam_items(&v);
        assert!(m["1"].price.is_none() && m["2"].price.is_none());
        let p = m["3"].price.as_ref().unwrap();
        assert_eq!((p.discount, p.fin, p.initial), (50, 5.0, 10.0)); // the missing original is derived, not $0
        assert_eq!(m["4"].price.as_ref().unwrap().discount, 100);
        let p = m["5"].price.as_ref().unwrap();
        assert_eq!((p.fin, p.initial), (0.0, 0.0));
    }

    #[test]
    fn gamerpower_zero_app_ids_and_noise_only_titles() {
        let body = r#"[{"id":1,"title":"Foo","status":"Active","platforms":"PC","steam_appid":0},
                       {"id":2,"title":"(Steam) Giveaway","status":"Active","platforms":"PC, Steam"},
                       {"id":3,"title":"   ","status":"Active","platforms":"PC"}]"#;
        let v = super::parse_gamerpower(body, "gp-", "free").unwrap();
        assert_eq!(v.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["gp-1", "gp-2"]);
        assert_eq!(v[0].app_id, None); // appid 0 is "not a Steam game"
        assert_eq!(v[1].title, "(Steam) Giveaway"); // better the raw text than an empty card
    }

    /// Every copy of `v` with one leaf (or one container) swapped for a value of another type.
    pub(crate) fn mutations(v: &serde_json::Value) -> Vec<serde_json::Value> {
        use serde_json::{json, Value};
        let swaps = [json!(null), json!(""), json!("NaN"), json!(-1), json!(1e300), json!(150), json!([]), json!({}), json!("1,234.56"), json!(true)];
        let mut out = vec![];
        match v {
            Value::Array(a) => for (i, x) in a.iter().enumerate() {
                for m in mutations(x) { let mut c = a.clone(); c[i] = m; out.push(Value::Array(c)); }
            },
            Value::Object(o) => for (k, x) in o {
                for m in mutations(x) { let mut c = o.clone(); c.insert(k.clone(), m); out.push(Value::Object(c)); }
            },
            _ => {}
        }
        out.extend(swaps.into_iter().filter(|s| s != v));
        out
    }

    fn sane_deals(what: &str, deals: &[super::Deal]) {
        for d in deals {
            assert!(d.price.is_finite() && d.price >= 0.0 && d.regular.is_finite() && d.regular >= 0.0 && d.cut <= 100, "{what}: {d:?}");
            assert!(!d.title.is_empty() && !d.id.is_empty(), "{what}: {d:?}");
        }
    }

    #[test]
    fn json_parsers_survive_wrong_types_and_keep_prices_sane() {
        use serde_json::json;
        let stores = std::collections::HashMap::from([("1".to_string(), "Steam".to_string())]);
        let gog = json!({"products": [{"id":"1","title":"W3","storeLink":"https://www.gog.com/en/game/w3","coverHorizontal":"https://i/c.jpg","price":{"finalMoney":{"amount":"5.00"},"baseMoney":{"amount":"10.00"},"discount":"-50%"}}]});
        for m in mutations(&gog) {
            sane_deals("gog", &super::parse_gog_sales(&m.to_string()).unwrap());
            super::parse_gog_product(&m, "W3");
        }
        let cs = json!([{"dealID":"A","gameID":"10","title":"Hades","storeID":"1","salePrice":"5.00","normalPrice":"20.00","savings":"75.6","steamAppID":"1145360","thumb":"t","steamRatingText":"Good","steamRatingPercent":"98","steamRatingCount":"250"}]);
        for m in mutations(&cs) {
            sane_deals("cheapshark", &super::parse_cheapshark_deals(&m.to_string(), &stores).unwrap());
            super::cheapshark_deals_by_game(&m.to_string());
            super::parse_cheapest_deal(&m, &stores);
            for (_, p, _, _) in super::cheapshark_matches(&[super::WishItem { app_id: Some("1145360".into()), ..Default::default() }], &[0], &m) {
                assert!(p.is_finite() && p >= 0.0, "{m}");
            }
        }
        let games = json!({"10": {"cheapestPriceEver": {"price": "4.25"}, "deals": [{"dealID": "a", "storeID": "1"}]}});
        for m in mutations(&games) {
            assert!(super::parse_cheapshark_lowest(&m.to_string()).values().all(|p| p.is_finite() && *p >= 0.0), "{m}");
            super::parse_deal_stores(&m.to_string());
        }
        let items = json!({"response": {"store_items": [{"appid": 620, "name": "Portal 2",
            "best_purchase_option": {"final_price_in_cents": "98", "original_price_in_cents": "979", "formatted_final_price": "$0.98", "discount_pct": 90},
            "assets": {"asset_url_format": "steam/apps/620/${FILENAME}", "header": "header.jpg"}}]}});
        for m in mutations(&items) {
            for it in super::parse_steam_items(&m).values() {
                if let Some(p) = &it.price { assert!(p.fin.is_finite() && p.fin >= 0.0 && p.initial.is_finite() && p.initial >= p.fin && p.discount <= 100, "{m}"); }
            }
        }
        let featured = json!({"large_capsules": [{"id": 1, "name": "Gift", "final_price": 0, "original_price": 1999, "large_capsule_image": "i"}], "featured_win": []});
        for m in mutations(&featured) { super::parse_steam_featured(&m); }
        let fdc = json!({"0": {"id": "cat_spotlight", "items": [{"name": "Free Weekend", "url": "https://store.steampowered.com/app/313120"}, {"name": "Autumn Sale", "url": "https://store.steampowered.com/sale/autumn2026"}]},
            "coming_soon": {"items": [{"id": 42, "discount_percent": 100}]}, "specials": {"items": [{"discount_percent": 60}]}});
        for m in mutations(&fdc) { super::steam_free_weekend_ids(&m); super::steam_sale_from(&m); }
        for m in mutations(&json!({"response": {"items": [{"appid": 10, "priority": 1, "date_added": 5}]}})) { super::parse_steam_wishlist_ids(&m); }
        let epic = json!({"data": {"Catalog": {"searchStore": {"elements": [{"title": "Control", "id": "o1", "productSlug": "control/home",
            "promotions": {"promotionalOffers": [{"promotionalOffers": [{"startDate": "s", "endDate": "e", "discountSetting": {"discountPercentage": 0}}]}],
                           "upcomingPromotionalOffers": [{"promotionalOffers": [{"startDate": "s", "discountSetting": {"discountPercentage": 0}}]}]},
            "keyImages": [{"type": "Thumbnail", "url": "u"}]}]}}}});
        for m in mutations(&epic) {
            for g in super::parse_epic_free(&m.to_string()).unwrap().iter().chain(&super::parse_epic_upcoming(&m.to_string()).unwrap()) {
                assert!(!g.title.is_empty() && g.url.starts_with("https://store.epicgames.com/"), "{m}");
            }
        }
        let gp = json!([{"id": 1, "title": "Hades (Steam) Giveaway", "status": "Active", "platforms": "PC, Steam", "end_date": "2026-10-24 23:59:59", "steam_appid": 5, "open_giveaway_url": "u", "thumbnail": "t"}]);
        for m in mutations(&gp) {
            for g in super::parse_gamerpower(&m.to_string(), "gp-", "free").unwrap() { assert!(!g.title.is_empty(), "{m}"); }
        }
    }

    #[test]
    fn html_parsers_survive_every_truncation() {
        let html = "<a data-ds-appid=\"1091500\"><img src=\"https://cdn/c.jpg\"><span class=\"title\">Hades &amp; Çé 🎮 \u{202e}עברית</span>\n\
<div data-discount=\"50\" data-price-final=\"50000\"><div class=\"discount_original_price\">$1,000.00</div></div></a>\
<a data-ds-appid=\"2\"><span class=\"title\">Free</span><div data-price-final=\"0\" data-discount=\"100\"><div class=\"discount_original_price\">$9.99</div></div></a>";
        for end in (0..=html.len()).filter(|&i| html.is_char_boundary(i)) {
            let body = search_body(&html[..end]);
            sane_deals("steam sales", &super::parse_steam_sales(&body).unwrap());
            for g in super::parse_steam_free_to_keep(&body).unwrap() { assert!(!g.title.is_empty()); }
        }
        let json_body = search_body(html);
        for end in (0..=json_body.len()).filter(|&i| json_body.is_char_boundary(i)) {
            let cut = &json_body[..end];
            let _ = (super::parse_steam_sales(cut), super::parse_steam_free_to_keep(cut), super::parse_gog_sales(cut), super::parse_epic_free(cut),
                     super::parse_epic_upcoming(cut), super::parse_gamerpower(cut, "gp-", "free"), super::read_steam_sale(cut),
                     super::appdetails_is_dlc(cut), super::parse_steam_profile(cut), super::profile_is_private(cut));
        }
        let title = "🎮".repeat(50_000);
        let v = super::parse_steam_sales(&search_body(&format!("<a data-ds-appid=\"1\"><span class=\"title\">{title}</span><div data-discount=\"10\" data-price-final=\"100\"></div></a>"))).unwrap();
        assert_eq!(v[0].title.chars().count(), 50_000);
    }

    #[test]
    fn numeric_and_named_entities_decode_once() {
        assert_eq!(super::unescape("Assassin&#039;s Creed &#x27;X&#X27; &apos;Y&apos;"), "Assassin's Creed 'X' 'Y'");
        assert_eq!(super::unescape("Caf&#233; &#x1F3AE; &nbsp;Go"), "Café 🎮  Go");
        assert_eq!(super::unescape("&#38;lt; &#x26;amp;"), "&lt; &amp;"); // an entity that decodes to "&" is not decoded again
        assert_eq!(super::unescape("a &bogus; b &#; &#xZZ; &#99999999; &#0; &#1; & &amp"), "a &bogus; b &#; &#xZZ; &#99999999; &#0; &#1; & &amp");
        assert_eq!(super::unescape("&#xD800; &#55296;"), "&#xD800; &#55296;"); // surrogates are not characters
    }
}
