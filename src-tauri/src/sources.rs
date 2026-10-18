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

fn f(v: &Value) -> f64 {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0.0)
}

fn s(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn unescape(t: &str) -> String {
    t.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
        .replace("&quot;", "\"").replace("&#39;", "'").replace("&trade;", "™")
        .replace("&reg;", "®").replace("&copy;", "©")
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
        let price = if bo.is_null() {
            None
        } else {
            let fin = f(&bo["final_price_in_cents"]) / 100.0;
            let discount = bo["discount_pct"].as_u64().unwrap_or(0) as u32;
            let initial = if discount > 0 { f(&bo["original_price_in_cents"]) / 100.0 } else { fin };
            Some(Price { discount, fin, initial, formatted: s(&bo["formatted_final_price"]), currency: Some("USD".into()) })
        };
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
        if cut == 0 { continue; }
        let price = p[1].parse::<f64>().unwrap_or(0.0) / 100.0;
        let regular = orig.captures(chunk)
            .and_then(|o| o[1].replace(',', "").parse().ok())
            .unwrap_or(price / (1.0 - cut as f64 / 100.0));
        out.push(Deal {
            id: format!("steam-{id}"),
            title: unescape(t[1].trim()),
            shop: "Steam".into(),
            url: format!("https://store.steampowered.com/app/{id}"),
            image: capsule.captures(chunk).map(|c| c[1].to_string()),
            cut, price, regular,
            lowest: None,
            app_id: Some(id.clone()),
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
fn gp_title(raw: &str) -> String {
    static NOISE: OnceLock<Regex> = OnceLock::new();
    let noise = re(r"(?i)\s*\((epic games|epic|steam|gog|gog\.com|free game)[^)]*\)|\s+giveaway$", &NOISE);
    noise.replace_all(raw, "").trim().to_string()
}

pub async fn gamerpower() -> R<Vec<Free>> {
    let body = get_text("https://www.gamerpower.com/api/giveaways?platform=pc&type=game").await?;
    parse_gamerpower(&body)
}

fn parse_gamerpower(body: &str) -> R<Vec<Free>> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    Ok(v.as_array().into_iter().flatten()
        .filter(|g| g["status"].as_str().is_none_or(|st| st.eq_ignore_ascii_case("active")))
        .filter_map(|g| {
            let plat = g["platforms"].as_str().unwrap_or("").to_lowercase();
            let source = if plat.contains("steam") { "steam" } else if plat.contains("epic") { "epic" }
                else if plat.contains("gog") { "gog" } else { "other" };
            let end = s(&g["end_date"]).filter(|e| e != "N/A");
            Some(Free {
                id: format!("gp-{}", g["id"].as_u64()?),
                title: gp_title(&s(&g["title"])?),
                source: source.into(),
                app_id: g["steam_appid"].as_u64().map(|n| n.to_string()),
                url: s(&g["open_giveaway_url"]).or(s(&g["open_giveaway"])).unwrap_or_else(|| "#".into()),
                end_date: end,
                image: s(&g["thumbnail"]),
                kind: "free".into(),
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
        let price = f(&p["price"]["finalMoney"]["amount"]);
        let regular = f(&p["price"]["baseMoney"]["amount"]);
        let cut = p["price"]["discount"].as_str()?.trim_start_matches('-').trim_end_matches('%').parse().ok()?;
        Some(Deal {
            id: format!("gog-{}", s(&p["id"])?),
            title: s(&p["title"])?,
            shop: "GOG".into(),
            url: s(&p["storeLink"])?,
            image: s(&p["coverHorizontal"]),
            cut, price, regular,
            lowest: None,
            app_id: None,
        })
    }).collect())
}

// ─── Hot deals: ITAD, with a keyless CheapShark fallback ──────────────────────

async fn cheapshark_stores() -> HashMap<String, String> {
    let Ok(v) = get_json("https://www.cheapshark.com/api/1.0/stores").await else { return HashMap::new() };
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
        get_text("https://www.cheapshark.com/api/1.0/deals?sortBy=Savings&onSale=1&pageSize=60&lowerPrice=1&metacritic=60&steamRating=70")
    );
    let body = body?;
    let mut deals = parse_cheapshark_deals(&body, &stores)?;
    let game_by_deal = cheapshark_deals_by_game(&body);
    let lowest = cheapshark_lowest(game_by_deal.values().cloned()).await;
    for deal in &mut deals {
        deal.lowest = game_by_deal.get(&deal.id).and_then(|id| lowest.get(id)).copied();
    }
    Ok(deals)
}

/// gameID → all-time low, 25 ids per request; failed batches are just missing.
async fn cheapshark_lowest(ids: impl IntoIterator<Item = String>) -> HashMap<String, f64> {
    let ids: Vec<String> = ids.into_iter().collect::<std::collections::HashSet<_>>().into_iter().collect();
    let results = futures::future::join_all(ids.chunks(25).map(|chunk| async move {
        get_text(&format!("https://www.cheapshark.com/api/1.0/games?ids={}", chunk.join(","))).await.ok()
    })).await;
    results.into_iter().flatten().flat_map(|body| parse_cheapshark_lowest(&body)).collect()
}

fn cheapshark_deals_by_game(body: &str) -> HashMap<String, String> {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return HashMap::new() };
    v.as_array().into_iter().flatten().filter_map(|d| Some((s(&d["dealID"])?, s(&d["gameID"])?))).collect()
}

fn parse_cheapshark_lowest(body: &str) -> HashMap<String, f64> {
    let Ok(v) = serde_json::from_str::<Value>(body) else { return HashMap::new() };
    v.as_object().into_iter().flatten().filter_map(|(id, game)| {
        game["cheapestPriceEver"]["price"].as_str()?.parse().ok().map(|price| (id.clone(), price))
    }).collect()
}

fn parse_cheapshark_deals(body: &str, stores: &HashMap<String, String>) -> R<Vec<Deal>> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    Ok(v.as_array().into_iter().flatten().filter_map(|d| {
        let id = s(&d["dealID"])?;
        Some(Deal {
            url: format!("https://www.cheapshark.com/redirect?dealID={id}"),
            id,
            title: s(&d["title"])?,
            shop: stores.get(s(&d["storeID"])?.as_str()).cloned().unwrap_or_else(|| "store".into()),
            image: s(&d["thumb"]),
            cut: f(&d["savings"]).round() as u32,
            price: f(&d["salePrice"]),
            regular: f(&d["normalPrice"]),
            lowest: None,
            app_id: s(&d["steamAppID"]).filter(|x| x != "0"),
        })
    }).collect())
}

/// Fill `cheapest` on wishlist items when another store beats the item's own
/// price (what deals.gg shows: the best current price across stores).
/// CheapShark batches Steam appids; GOG/Epic items are matched by exact title.
pub async fn attach_cheapest(items: &mut [WishItem]) {
    let stores = cheapshark_stores().await;
    let mut found: Vec<GameMatch> = vec![];

    // CheapShark prices are USD, so only USD-priced items are compared (GOG may use the account's currency).
    let usd = |w: &WishItem| w.price_info.as_ref().is_some_and(|p| p.currency.as_deref() == Some("USD"));
    // ponytail: at most 250 Steam games (10 requests), discounted first; huge wishlists got CheapShark 429s.
    // Rotate or cache per app id if users want "cheaper elsewhere" on every item of a 1000+ wishlist.
    let mut steam_idx: Vec<usize> = (0..items.len()).filter(|&i| items[i].app_id.is_some() && usd(&items[i])).collect();
    steam_idx.sort_by_key(|&i| std::cmp::Reverse(items[i].price_info.as_ref().map_or(0, |p| p.discount)));
    steam_idx.truncate(250);
    for chunk in steam_idx.chunks(25) {
        let ids: Vec<&str> = chunk.iter().filter_map(|&i| items[i].app_id.as_deref()).collect();
        let Ok(v) = get_json(&format!("https://www.cheapshark.com/api/1.0/games?steamAppID={}", ids.join(","))).await else { continue };
        found.extend(cheapshark_matches(items, chunk, &v));
    }
    let other_idx: Vec<usize> = (0..items.len()).filter(|&i| items[i].app_id.is_none() && usd(&items[i])).take(30).collect();
    let lookups = futures::future::join_all(other_idx.iter().map(|&i| {
        let url = format!("https://www.cheapshark.com/api/1.0/games?title={}&exact=1&limit=1", urlenc(&items[i].name));
        async move { (i, get_json(&url).await.ok()) }
    })).await;
    for (i, v) in lookups {
        if let Some(v) = v { found.extend(cheapshark_matches(items, &[i], &v)); }
    }

    // Lowest-ever only matters (and is only shown) for games on sale
    let games: Vec<(usize, String)> = found.iter()
        .filter(|m| items[m.0].price_info.as_ref().is_some_and(|p| p.discount > 0))
        .filter_map(|m| Some((m.0, m.3.clone()?))).collect();
    // Only look up store/link for games where somebody else is strictly cheaper
    let cheaper: Vec<_> = found.into_iter()
        .filter_map(|(i, price, deal, _)| Some((i, price, deal?)))
        .filter(|(i, price, _)| items[*i].price_info.as_ref().is_some_and(|p| *price + 0.005 < p.fin))
        .collect();
    let (lowest, details) = futures::join!(
        cheapshark_lowest(games.iter().map(|(_, g)| g.clone())),
        futures::future::join_all(cheaper.iter().map(|(_, _, deal)| async move {
            get_json(&format!("https://www.cheapshark.com/api/1.0/deals?id={deal}")).await.ok()
        }))
    );
    assign_lowest(items, &games, &lowest);
    for ((i, price, deal), d) in cheaper.into_iter().zip(details) {
        let store = d.and_then(|d| s(&d["gameInfo"]["storeID"])).and_then(|id| stores.get(&id).cloned());
        if let Some(store) = store {
            items[i].cheapest = Some(Cheapest { store, price, url: format!("https://www.cheapshark.com/redirect?dealID={deal}") });
        }
    }
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
        Some((i, f(&g["cheapest"]), s(&g["cheapestDealID"]), s(&g["gameID"])))
    }).collect()
}

fn assign_lowest(items: &mut [WishItem], games: &[(usize, String)], lowest: &HashMap<String, f64>) {
    for (i, game) in games { items[*i].lowest = lowest.get(game).copied(); }
}

/// Cheapest third-party price for one Steam game (wishlist detail panel).
pub async fn cheapest_for_steam_app(app_id: &str) -> R<Option<Value>> {
    let url = format!("https://www.cheapshark.com/api/1.0/deals?steamAppID={}&pageSize=15", urlenc(app_id));
    let (stores, deals) = futures::join!(cheapshark_stores(), get_json(&url));
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
    #[tokio::test] #[ignore] async fn gamerpower_live() { show("gamerpower", gamerpower().await) }
    #[tokio::test] #[ignore] async fn cheapshark_live() {
        let deals = cheapshark_deals().await.expect("CheapShark deals");
        assert!(deals.iter().any(|deal| deal.lowest.is_some()), "no deals included a lowest-ever price");
        show("cheapshark", Ok(deals));
    }
    #[tokio::test] #[ignore] async fn wishlist_cheapest_live() {
        // STEAM_ID=7656... cargo test wishlist_cheapest_live -- --ignored --nocapture
        let id = std::env::var("STEAM_ID").expect("set STEAM_ID");
        let mut w = steam_wishlist(&id).await.unwrap();
        attach_cheapest(&mut w).await;
        let n = w.iter().filter(|i| i.cheapest.is_some()).count();
        println!("{} items, {} have a cheaper non-Steam offer, {} have a lowest-ever price", w.len(), n, w.iter().filter(|i| i.lowest.is_some()).count());
        for i in w.iter().filter(|i| i.cheapest.is_some()).take(5) {
            println!("{} steam ${:.2} -> {:?}", i.name, i.price_info.as_ref().unwrap().fin, i.cheapest);
        }
    }
    #[tokio::test] #[ignore] async fn steam_wishlist_prices_live() {
        let ids = vec!["60".to_string(), "1203620".to_string()];
        let m = steam_items(&ids).await.unwrap();
        assert_eq!(m.len(), 2);
        println!("{:?}", m);
    }
}

#[cfg(test)]
mod unit {
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
        let v = super::parse_gamerpower(body).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "gp-3100");
        assert_eq!(v[0].title, "Hades");
        assert_eq!(v[0].source, "steam");
        assert_eq!(v[0].app_id.as_deref(), Some("1145360"));
        assert_eq!(v[0].end_date.as_deref(), Some("2026-10-24 23:59:59"));
        assert_eq!(v[0].url, "https://www.gamerpower.com/open/hades");
        assert_eq!(v[0].image.as_deref(), Some("https://www.gamerpower.com/thumb.jpg"));
        assert_eq!(v[0].kind, "free");
        assert!(super::parse_gamerpower("nope").is_err());
        assert!(super::parse_gamerpower(r#"{"status":0}"#).unwrap().is_empty());
    }

    #[test]
    fn cheapshark_parses_deals() {
        let body = r#"[
            {"dealID":"AbC","gameID":"10","title":"Hades","storeID":"1","salePrice":"5.00","normalPrice":"20.00","savings":"75.6","steamAppID":"1145360","thumb":"https://cdn.example/t.jpg"},
            {"dealID":"XyZ","gameID":"11","title":"Loose","storeID":"9","salePrice":"3.00","normalPrice":"3.00","savings":"0","steamAppID":"0"}
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
        assert!(super::cheapshark_deals_by_game("not json").is_empty());
    }
}
