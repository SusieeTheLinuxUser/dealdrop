//! Every outbound API call lives here. Each fetcher returns `Result<_, String>`
//! so the poll loop can show *why* a source is empty instead of failing silently.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

pub type R<T> = Result<T, String>;

const UA: &str = "DealDrop/0.7.9 (+https://github.com/SusieeTheLinuxUser/dealdrop)";
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

async fn get_json_browser(url: &str) -> R<Value> {
    send(client().get(url).header("User-Agent", UA_BROWSER).header("Accept", "application/json"))
        .await?
        .json()
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
        let v = get_json(&url).await?;
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
                Some(Price { discount, fin, initial, formatted: s(&bo["formatted_final_price"]) })
            };
            let a = &it["assets"];
            let image = a["asset_url_format"].as_str()
                .zip(a["header"].as_str().or(a["small_capsule"].as_str()))
                .map(|(fmt, file)| format!("https://shared.akamai.steamstatic.com/store_item_assets/{}", fmt.replace("${FILENAME}", file)))
                .unwrap_or_else(|| steam_header(&id));
            out.insert(id, SteamItem { name, price, image });
        }
    }
    Ok(out)
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

/// A seasonal sale that is live right now, if the store is advertising one.
/// Returns (generic "sale is on" flag, themed sale if recognised).
pub async fn detect_steam_sale() -> R<(bool, Option<LiveSale>)> {
    let fdc = featured_categories().await?;
    let heavy = fdc["specials"]["items"].as_array().into_iter().flatten()
        .filter(|i| i["discount_percent"].as_u64().unwrap_or(0) >= 50).count();
    let live = sale_from_banners(spotlights(&fdc));
    Ok((live.is_some() || heavy >= 8, live))
}

pub async fn steam_free_featured() -> R<Vec<Free>> {
    let v = get_json("https://store.steampowered.com/api/featured/?cc=us&l=en").await?;
    let items = v["large_capsules"].as_array().into_iter().flatten()
        .chain(v["featured_win"].as_array().into_iter().flatten());
    Ok(items
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
        .collect())
}

pub async fn steam_free_weekends() -> R<Vec<Free>> {
    static APP: OnceLock<Regex> = OnceLock::new();
    let app_re = re(r"/app/(\d+)", &APP);
    let fdc = featured_categories().await?;

    let mut ids: Vec<String> = spotlights(&fdc).iter()
        .filter(|b| b["name"].as_str().is_some_and(|n| n.to_lowercase().contains("free weekend")))
        .filter_map(|b| app_re.captures(b["url"].as_str()?).map(|c| c[1].to_string()))
        .collect();
    // Legacy shape: coming_soon items at 100% off
    ids.extend(fdc["coming_soon"]["items"].as_array().into_iter().flatten()
        .filter(|i| i["discount_percent"] == 100)
        .filter_map(|i| i["id"].as_u64().map(|n| n.to_string())));
    ids.sort();
    ids.dedup();

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

/// Paid apps at 100% off (`specials=1&maxprice=free`). Rows are HTML inside JSON.
/// Id prefix `steam-ftk-` so a failure restore does not collide with `steam-{appid}`.
pub async fn steam_free_to_keep() -> R<Vec<Free>> {
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

    let v = get_json("https://store.steampowered.com/search/results/?query&start=0&count=50&specials=1&maxprice=free&infinite=1&cc=us&l=en&json=1").await?;
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
            title: unescape(&t[1]),
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
    static ROW: OnceLock<Regex> = OnceLock::new();
    static TITLE: OnceLock<Regex> = OnceLock::new();
    static DISC: OnceLock<Regex> = OnceLock::new();
    static FINAL: OnceLock<Regex> = OnceLock::new();
    static ORIG: OnceLock<Regex> = OnceLock::new();
    let row = re(r#"data-ds-appid="(\d+)""#, &ROW);
    let title = re(r#"<span class="title">([^<]*)</span>"#, &TITLE);
    let disc = re(r#"data-discount="(\d+)""#, &DISC);
    let fin = re(r#"data-price-final="(\d+)""#, &FINAL);
    let orig = re(r#"discount_original_price">\$?([\d.,]+)<"#, &ORIG);

    let v = get_json("https://store.steampowered.com/search/results/?query&start=0&count=50&specials=1&infinite=1&cc=us&l=en&json=1").await?;
    let html = v["results_html"].as_str().ok_or("no results_html")?;
    static CAPSULE: OnceLock<Regex> = OnceLock::new();
    let capsule = re(r#"<img src="([^"]+)""#, &CAPSULE);
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
            title: unescape(&t[1]),
            shop: "Steam".into(),
            url: format!("https://store.steampowered.com/app/{id}"),
            image: capsule.captures(chunk).map(|c| c[1].to_string()),
            cut, price, regular,
            app_id: Some(id.clone()),
        });
    }
    // Replace the tiny search capsules with proper header art where we can
    let ids: Vec<String> = out.iter().filter_map(|d| d.app_id.clone()).collect();
    if let Ok(items) = steam_items(&ids).await {
        for d in &mut out {
            if let Some(i) = d.app_id.as_ref().and_then(|a| items.get(a)) { d.image = Some(i.image.clone()); }
        }
    }
    Ok(out)
}

// ─── Steam: wishlist ──────────────────────────────────────────────────────────

pub async fn steam_wishlist(steam_id: &str) -> R<Vec<WishItem>> {
    let v = get_json(&format!(
        "https://api.steampowered.com/IWishlistService/GetWishlist/v1?steamid={steam_id}"
    )).await?;
    let mut raw: Vec<(String, u64, u64)> = v["response"]["items"].as_array().into_iter().flatten()
        .filter_map(|i| Some((i["appid"].as_u64()?.to_string(), i["priority"].as_u64().unwrap_or(0), i["date_added"].as_u64().unwrap_or(0))))
        .collect();
    // priority 0 means "unranked" — user-ranked entries first, then newest
    raw.sort_by_key(|(_, p, added)| (*p == 0, *p, std::cmp::Reverse(*added)));

    let ids: Vec<String> = raw.iter().map(|r| r.0.clone()).collect();
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
            id,
        }
    }).collect())
}

/// Distinguishes "wishlist is empty" from "we can't see it".
pub async fn steam_profile_private(steam_id: &str) -> bool {
    get_text(&format!("https://steamcommunity.com/profiles/{steam_id}/?xml=1")).await
        .map(|x| x.to_lowercase().contains("<privacystate>private</privacystate>"))
        .unwrap_or(false)
}

/// Returns (name, avatar) from the public profile XML (no API key needed).
pub async fn steam_profile(steam_id: &str) -> (String, Option<String>) {
    static NAME: OnceLock<Regex> = OnceLock::new();
    static AVATAR: OnceLock<Regex> = OnceLock::new();
    let name_re = re(r"<steamID>(?:<!\[CDATA\[)?([^\]<]+)", &NAME);
    let avatar_re = re(r"<avatarMedium>(?:<!\[CDATA\[)?([^\]<]+)", &AVATAR);
    if let Ok(x) = get_text(&format!("https://steamcommunity.com/profiles/{steam_id}/?xml=1")).await {
        if let Some(n) = name_re.captures(&x) {
            return (n[1].trim().to_string(), avatar_re.captures(&x).map(|a| a[1].trim().to_string()));
        }
    }
    let tail = steam_id.char_indices().nth_back(3).map(|(i, _)| &steam_id[i..]).unwrap_or(steam_id);
    (format!("Steam user {tail}"), None)
}

// ─── Free games: Epic + GamerPower ────────────────────────────────────────────

pub async fn epic_free() -> R<Vec<Free>> {
    let v = get_json_browser("https://store-site-backend-static.ak.epicgames.com/freeGamesPromotions?locale=en-US&country=US&allowCountries=US").await?;
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
                image: g["keyImages"].as_array().and_then(|k| k.iter().find(|i| i["type"] == "Thumbnail")).and_then(|i| s(&i["url"])),
                kind: "free".into(),
                ..Default::default()
            })
        })
        .collect())
}

pub fn epic_url(g: &Value) -> String {
    let slug = s(&g["productSlug"])
        .or_else(|| s(&g["catalogNs"]["mappings"][0]["pageSlug"]))
        .or_else(|| s(&g["offerMappings"][0]["pageSlug"]))
        .or_else(|| s(&g["urlSlug"]))
        .filter(|x| !x.is_empty() && x != "[]");
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
    let v = get_json("https://www.gamerpower.com/api/giveaways?platform=pc&type=game").await?;
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
            })
        })
        .collect())
}

// ─── GOG sales ────────────────────────────────────────────────────────────────

pub async fn gog_sales() -> R<Vec<Deal>> {
    let v = get_json("https://catalog.gog.com/v1/catalog?limit=48&order=desc:trending&discounted=eq:true&productType=in:game,pack&page=1&countryCode=US&locale=en-US&currencyCode=USD").await?;
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
            app_id: None,
        })
    }).collect())
}

// ─── Hot deals: ITAD, with a keyless CheapShark fallback ──────────────────────

async fn cheapshark_stores() -> HashMap<String, String> {
    let Ok(v) = get_json("https://www.cheapshark.com/api/1.0/stores").await else { return HashMap::new() };
    v.as_array().into_iter().flatten()
        .filter_map(|x| Some((s(&x["storeID"])?, s(&x["storeName"])?)))
        .collect()
}

/// No key needed: CheapShark aggregates ~30 stores and sorts by savings.
pub async fn cheapshark_deals() -> R<Vec<Deal>> {
    let (stores, v) = futures::join!(
        cheapshark_stores(),
        get_json("https://www.cheapshark.com/api/1.0/deals?sortBy=Savings&onSale=1&pageSize=60&lowerPrice=1&metacritic=60&steamRating=70")
    );
    Ok(v?.as_array().into_iter().flatten().filter_map(|d| {
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
            app_id: s(&d["steamAppID"]).filter(|x| x != "0"),
        })
    }).collect())
}

/// Fill `cheapest` on wishlist items when another store beats the item's own
/// price (what deals.gg shows: the best current price across stores).
/// CheapShark batches Steam appids; GOG/Epic items are matched by exact title.
pub async fn attach_cheapest(items: &mut [WishItem]) {
    let stores = cheapshark_stores().await;
    // (item index, cheapest price, dealID)
    let mut found: Vec<(usize, f64, String)> = vec![];

    let steam_idx: Vec<usize> = (0..items.len()).filter(|&i| items[i].app_id.is_some() && items[i].price_info.is_some()).collect();
    for chunk in steam_idx.chunks(25) {
        let ids: Vec<&str> = chunk.iter().filter_map(|&i| items[i].app_id.as_deref()).collect();
        let Ok(v) = get_json(&format!("https://www.cheapshark.com/api/1.0/games?steamAppID={}", ids.join(","))).await else { continue };
        for g in v.as_array().into_iter().flatten() {
            let app = s(&g["steamAppID"]).unwrap_or_default();
            if let (Some(&i), Some(deal)) = (chunk.iter().find(|&&i| items[i].app_id.as_deref() == Some(app.as_str())), s(&g["cheapestDealID"])) {
                found.push((i, f(&g["cheapest"]), deal));
            }
        }
    }
    let other_idx: Vec<usize> = (0..items.len()).filter(|&i| items[i].app_id.is_none() && items[i].price_info.is_some()).take(30).collect();
    let lookups = futures::future::join_all(other_idx.iter().map(|&i| {
        let url = format!("https://www.cheapshark.com/api/1.0/games?title={}&exact=1&limit=1", urlenc(&items[i].name));
        async move { (i, get_json(&url).await.ok()) }
    })).await;
    for (i, v) in lookups {
        if let Some(g) = v.as_ref().and_then(|v| v.get(0)) {
            if let Some(deal) = s(&g["cheapestDealID"]) { found.push((i, f(&g["cheapest"]), deal)); }
        }
    }

    // Only look up store/link for games where somebody else is strictly cheaper
    let cheaper: Vec<_> = found.into_iter()
        .filter(|(i, price, _)| items[*i].price_info.as_ref().is_some_and(|p| *price + 0.005 < p.fin))
        .collect();
    let details = futures::future::join_all(cheaper.iter().map(|(_, _, deal)| async move {
        get_json(&format!("https://www.cheapshark.com/api/1.0/deals?id={deal}")).await.ok()
    })).await;
    for ((i, price, deal), d) in cheaper.into_iter().zip(details) {
        let store = d.and_then(|d| s(&d["gameInfo"]["storeID"])).and_then(|id| stores.get(&id).cloned());
        if let Some(store) = store {
            items[i].cheapest = Some(Cheapest { store, price, url: format!("https://www.cheapshark.com/redirect?dealID={deal}") });
        }
    }
}

/// Cheapest third-party price for one Steam game (wishlist detail panel).
pub async fn cheapest_for_steam_app(app_id: &str) -> R<Option<Value>> {
    let url = format!("https://www.cheapshark.com/api/1.0/deals?steamAppID={}&pageSize=15", urlenc(app_id));
    let (stores, deals) = futures::join!(cheapshark_stores(), get_json(&url));
    let deals = deals?;
    let best = deals.as_array().into_iter().flatten()
        .filter(|d| f(&d["salePrice"]) > 0.0)
        .min_by(|a, b| f(&a["salePrice"]).total_cmp(&f(&b["salePrice"])));
    Ok(best.map(|b| {
        let deal_id = s(&b["dealID"]);
        json!({
            "store": stores.get(s(&b["storeID"]).unwrap_or_default().as_str()).cloned().unwrap_or_else(|| "store".into()),
            "price": f(&b["salePrice"]),
            "retailPrice": f(&b["normalPrice"]),
            "savingsPercent": f(&b["savings"]).round(),
            "url": deal_id.map(|d| format!("https://www.cheapshark.com/redirect?dealID={d}")),
        })
    }))
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
    #[tokio::test] #[ignore] async fn epic_sales_live() { show("epic sales", crate::stores::epic_sales().await) }
    #[tokio::test] #[ignore] async fn epic_free_live() { show("epic free", epic_free().await) }
    #[tokio::test] #[ignore] async fn gamerpower_live() { show("gamerpower", gamerpower().await) }
    #[tokio::test] #[ignore] async fn cheapshark_live() { show("cheapshark", cheapshark_deals().await) }
    #[tokio::test] #[ignore] async fn wishlist_cheapest_live() {
        // STEAM_ID=7656... cargo test wishlist_cheapest_live -- --ignored --nocapture
        let id = std::env::var("STEAM_ID").expect("set STEAM_ID");
        let mut w = steam_wishlist(&id).await.unwrap();
        attach_cheapest(&mut w).await;
        let n = w.iter().filter(|i| i.cheapest.is_some()).count();
        println!("{} items, {} have a cheaper non-Steam offer", w.len(), n);
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
    fn gamerpower_titles_are_cleaned() {
        assert_eq!(super::gp_title("Agent A: A puzzle in disguise (Epic Games) Giveaway"), "Agent A: A puzzle in disguise");
        assert_eq!(super::gp_title("Bad Cheese (Epic Games)"), "Bad Cheese");
        assert_eq!(super::gp_title("Foo (Steam) Giveaway"), "Foo");
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
}
