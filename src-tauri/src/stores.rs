//! Account features for the non-Steam stores (GOG, Epic) plus the sign-in
//! webview shared by all three stores, and Epic's discounted-games list.

use crate::sources::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tokio::sync::mpsc;

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64, // ms since epoch
    pub name: String,
    #[serde(default)]
    pub avatar: Option<String>,
}

impl Tokens {
    pub fn stale(&self) -> bool {
        now_ms() + 60_000 > self.expires_at
    }
}

// ─── Sign-in window ───────────────────────────────────────────────────────────

/// Opens `start` in a window and resolves with the first URL for which
/// `is_callback` is true. That navigation is cancelled, so the redirect target
/// never has to exist. Errors with "Cancelled" if the user closes the window.
pub async fn sign_in_window(
    app: &AppHandle,
    label: &str,
    title: &str,
    start: &str,
    init_script: Option<&str>,
    is_callback: fn(&str) -> bool,
) -> R<url::Url> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Option<url::Url>>();
    let tx_nav = tx.clone();
    let mut b = WebviewWindowBuilder::new(app, label, WebviewUrl::External(start.parse().map_err(|e: url::ParseError| e.to_string())?))
        .title(title)
        .inner_size(820.0, 660.0)
        .on_navigation(move |u| {
            if is_callback(u.as_str()) {
                let _ = tx_nav.send(Some(u.clone()));
                false
            } else {
                u.scheme() == "https" // sign-in pages never need anything else
            }
        });
    if let Some(js) = init_script {
        b = b.initialization_script(js);
    }
    let win = b.build().map_err(|e| e.to_string())?;
    win.on_window_event(move |ev| {
        if matches!(ev, WindowEvent::Destroyed) {
            let _ = tx.send(None);
        }
    });
    let res = rx.recv().await.flatten().ok_or_else(|| "Cancelled".to_string());
    let _ = win.destroy();
    res
}

// ─── Steam OpenID ─────────────────────────────────────────────────────────────

const STEAM_CB: &str = "http://localhost/steamcallback";

pub async fn steam_login(app: &AppHandle) -> R<String> {
    let q = [
        ("openid.mode", "checkid_setup"),
        ("openid.ns", "http://specs.openid.net/auth/2.0"),
        ("openid.identity", "http://specs.openid.net/auth/2.0/identifier_select"),
        ("openid.claimed_id", "http://specs.openid.net/auth/2.0/identifier_select"),
        ("openid.return_to", STEAM_CB),
        ("openid.realm", "http://localhost/"),
    ];
    let start = format!("https://steamcommunity.com/openid/login?{}", url::form_urlencoded::Serializer::new(String::new()).extend_pairs(q).finish());
    let cb = sign_in_window(app, "steam-login", "Sign in through Steam", &start, None, |u| u.starts_with(STEAM_CB)).await?;

    // Don't trust the query string: ask Steam to confirm it signed this assertion.
    let mut form: Vec<(String, String)> = cb.query_pairs().map(|(k, v)| (k.into(), v.into())).collect();
    form.retain(|(k, _)| k != "openid.mode");
    form.push(("openid.mode".into(), "check_authentication".into()));
    let verdict = client().post("https://steamcommunity.com/openid/login").form(&form)
        .send().await.map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string())?;
    if !verdict.contains("is_valid:true") {
        return Err("Steam did not confirm the sign-in".into());
    }
    let claimed = cb.query_pairs().find(|(k, _)| k == "openid.claimed_id").map(|(_, v)| v.to_string()).unwrap_or_default();
    if !claimed.starts_with("https://steamcommunity.com/openid/id/") { return Err("Steam callback had an unexpected identity".into()); }
    claimed.rsplit('/').next().filter(|id| id.len() >= 17 && id.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string).ok_or_else(|| "Steam callback missing claimed_id".into())
}

// ─── GOG ──────────────────────────────────────────────────────────────────────
// Same public Galaxy-client OAuth app that Heroic/Lutris/minigalaxy use. The
// client id/secret identify "GOG Galaxy", not the user, and are not private.

const GOG_ID: &str = "46899977096215655";
const GOG_SECRET: &str = "9d85c43b1482497dbbce61f6e4aa173a433796eeae2ca8c5f6129f2dc4de46d9";
const GOG_REDIRECT: &str = "https://embed.gog.com/on_login_success?origin=client";

async fn gog_token(extra: &[(&str, &str)]) -> R<Tokens> {
    let mut form = vec![("client_id", GOG_ID), ("client_secret", GOG_SECRET)];
    form.extend_from_slice(extra);
    let v: Value = client().post("https://auth.gog.com/token").form(&form)
        .send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
    if let Some(e) = v["error_description"].as_str().or(v["error"].as_str()) {
        return Err(format!("GOG: {e}"));
    }
    Ok(Tokens {
        access_token: v["access_token"].as_str().ok_or("GOG: no access_token")?.into(),
        refresh_token: v["refresh_token"].as_str().unwrap_or_default().into(),
        expires_at: now_ms() + v["expires_in"].as_u64().unwrap_or(3600) * 1000,
        name: String::new(),
        avatar: None,
    })
}

pub async fn gog_login(app: &AppHandle) -> R<Tokens> {
    let start = format!(
        "https://auth.gog.com/auth?client_id={GOG_ID}&redirect_uri={}&response_type=code&layout=client2",
        url::form_urlencoded::byte_serialize(GOG_REDIRECT.as_bytes()).collect::<String>()
    );
    let cb = sign_in_window(app, "gog-login", "Sign in to GOG", &start, None, |u| u.starts_with("https://embed.gog.com/on_login_success")).await?;
    let code = cb.query_pairs().find(|(k, _)| k == "code").map(|(_, v)| v.to_string()).ok_or("GOG callback had no code")?;
    let mut t = gog_token(&[("grant_type", "authorization_code"), ("code", &code), ("redirect_uri", GOG_REDIRECT)]).await?;
    let u = gog_user(&t.access_token).await;
    t.name = u.0;
    t.avatar = u.3;
    Ok(t)
}

pub async fn gog_refresh(t: &Tokens) -> R<Tokens> {
    let mut n = gog_token(&[("grant_type", "refresh_token"), ("refresh_token", &t.refresh_token)]).await?;
    if n.refresh_token.is_empty() { n.refresh_token = t.refresh_token.clone(); }
    n.name = t.name.clone();
    n.avatar = t.avatar.clone();
    Ok(n)
}

/// (username, country, currency, avatar url) — falls back to US/USD.
pub async fn gog_user(access: &str) -> (String, String, String, Option<String>) {
    let v = send_auth("https://embed.gog.com/userData.json", access).await.unwrap_or(Value::Null);
    (
        v["username"].as_str().unwrap_or("GOG user").into(),
        v["country"].as_str().filter(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_alphabetic())).unwrap_or("US").into(), // goes into a URL: letters only
        v["selectedCurrency"]["code"].as_str().filter(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_alphabetic())).unwrap_or("USD").into(),
        v["avatar"].as_str().filter(|a| a.starts_with("https://images.gog.com/")).map(|a| format!("{a}_avm.jpg")),
    )
}

async fn send_auth(url: &str, access: &str) -> R<Value> {
    let res = client().get(url).bearer_auth(access).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status().as_u16()));
    }
    res.json().await.map_err(|e| e.to_string())
}

pub async fn gog_wishlist(access: &str) -> R<Vec<WishItem>> {
    let v = send_auth("https://embed.gog.com/user/wishlist.json", access).await?;
    let ids: Vec<String> = v["wishlist"].as_object().into_iter().flatten()
        .filter(|(_, on)| **on != false && **on != 0).map(|(k, _)| k.clone()).collect();
    if ids.is_empty() { return Ok(vec![]); }
    let (_, country, currency, _) = gog_user(access).await;

    let mut out = vec![];
    for chunk in ids.chunks(50) {
        let list = chunk.join(",");
        let info_url = format!("https://api.gog.com/products?ids={list}");
        let price_url = format!("https://api.gog.com/products/prices?ids={list}&countryCode={country}&currency={currency}");
        let (info, prices) = futures::join!(get_json(&info_url), get_json(&price_url));
        let prices = prices.unwrap_or(Value::Null);
        for p in info?.as_array().into_iter().flatten() {
            let id = p["id"].as_u64().map(|n| n.to_string()).unwrap_or_default();
            let price = prices["_embedded"]["items"].as_array().into_iter().flatten()
                .find(|i| i["_embedded"]["product"]["id"].as_u64().map(|n| n.to_string()).as_deref() == Some(&id))
                .and_then(|i| {
                    let pr = &i["_embedded"]["prices"][0];
                    let cents = |k: &str| pr[k].as_str()?.split_whitespace().next()?.parse::<f64>().ok().map(|c| c / 100.0);
                    let (base, fin) = (cents("basePrice")?, cents("finalPrice")?);
                    Some(Price {
                        discount: if base > 0.0 { ((1.0 - fin / base) * 100.0).round() as u32 } else { 0 },
                        fin, initial: base,
                        formatted: Some(if fin == 0.0 { "Free".into() } else { format!("${fin:.2}") }),
                    })
                });
            out.push(WishItem {
                store: "gog".into(),
                url: format!("https://www.gog.com/en/game/{}", p["slug"].as_str().unwrap_or_default()),
                image: p["images"]["logo2x"].as_str().map(|u| format!("https:{u}")),
                name: p["title"].as_str().unwrap_or("GOG game").into(),
                id, app_id: None, price_info: price, cheapest: None,
            });
        }
    }
    Ok(out)
}

// ─── Epic Games ───────────────────────────────────────────────────────────────
// Launcher OAuth client (same one legendary/Heroic use), then the store's
// GraphQL API with that bearer token.

const EPIC_BASIC: &str = "MzRhMDJjZjhmNDQxNGUyOWIxNTkyMTg3NmRhMzZmOWE6ZGFhZmJjY2M3Mzc3NDUwMzlkZmZlNTNkOTRmYzc2Y2Y=";
const EPIC_UA_LAUNCHER: &str = "EpicGamesLauncher/14.0.8-22004686+++Portal+Release-Live";
const EPIC_UA_BROWSER: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0";

// Epic returns the authorization code as a JSON document. The page can't hand
// it to us directly, so this script reads it and "navigates" to a URL that
// sign_in_window intercepts.
const EPIC_JS: &str = r#"
document.addEventListener('DOMContentLoaded', () => {
  if (location.hostname !== 'www.epicgames.com' || location.pathname !== '/id/api/redirect') return
  try {
    const code = JSON.parse(document.body.innerText).authorizationCode
    if (code) location.href = 'https://dealdrop.invalid/epic?code=' + encodeURIComponent(code)
  } catch (e) {}
})"#;

async fn epic_token(extra: &[(&str, &str)]) -> R<Tokens> {
    let mut form = vec![("token_type", "eg1")];
    form.extend_from_slice(extra);
    let v: Value = client()
        .post("https://account-public-service-prod03.ol.epicgames.com/account/api/oauth/token")
        .header("Authorization", format!("basic {EPIC_BASIC}"))
        .form(&form)
        .send().await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
    if let Some(e) = v["errorMessage"].as_str() {
        return Err(format!("Epic: {e}"));
    }
    Ok(Tokens {
        access_token: v["access_token"].as_str().ok_or("Epic: no access_token")?.into(),
        refresh_token: v["refresh_token"].as_str().unwrap_or_default().into(),
        expires_at: now_ms() + v["expires_in"].as_u64().unwrap_or(3600) * 1000,
        name: v["displayName"].as_str().unwrap_or("Epic user").into(),
        avatar: None, // Epic exposes no public avatar
    })
}

pub async fn epic_login(app: &AppHandle) -> R<Tokens> {
    let start = "https://www.epicgames.com/id/login?redirectUrl=https%3A//www.epicgames.com/id/api/redirect%3FclientId%3D34a02cf8f4414e29b15921876da36f9a%26responseType%3Dcode";
    let cb = sign_in_window(app, "epic-login", "Sign in to Epic Games", start, Some(EPIC_JS), |u| u.starts_with("https://dealdrop.invalid/")).await?;
    let code = cb.query_pairs().find(|(k, _)| k == "code").map(|(_, v)| v.to_string()).ok_or("Epic callback had no code")?;
    epic_token(&[("grant_type", "authorization_code"), ("code", &code)]).await
}

pub async fn epic_refresh(t: &Tokens) -> R<Tokens> {
    let mut n = epic_token(&[("grant_type", "refresh_token"), ("refresh_token", &t.refresh_token)]).await?;
    if n.name.is_empty() { n.name = t.name.clone(); }
    Ok(n)
}

const EPIC_OFFER_FIELDS: &str = r#"title id namespace urlSlug productSlug
  keyImages{type url}
  catalogNs{mappings(pageType:"productHome"){pageSlug pageType}}
  offerMappings{pageSlug pageType}
  price(country:"US"){totalPrice{discountPrice originalPrice discount currencyInfo{decimals}
    fmtPrice(locale:"en-US"){discountPrice}}}"#;

async fn epic_gql(url: &str, ua: &str, bearer: Option<&str>, query: &str, vars: Value) -> R<Value> {
    let mut req = client().post(url).header("User-Agent", ua).json(&json!({"query": query, "variables": vars}));
    if let Some(b) = bearer { req = req.bearer_auth(b); }
    let res = req.send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status().as_u16()));
    }
    let v: Value = res.json().await.map_err(|e| e.to_string())?;
    if let Some(e) = v["errors"][0]["message"].as_str() {
        if v["data"].is_null() { return Err(e.to_string()); }
    }
    Ok(v)
}

fn epic_image(o: &Value) -> Option<String> {
    let imgs = o["keyImages"].as_array()?;
    ["OfferImageWide", "OfferImageTall", "Thumbnail"].iter()
        .find_map(|t| imgs.iter().find(|i| i["type"] == *t))
        .and_then(|i| i["url"].as_str().map(str::to_string))
}

fn epic_page(o: &Value) -> String {
    let slug = [&o["offerMappings"][0]["pageSlug"], &o["catalogNs"]["mappings"][0]["pageSlug"], &o["productSlug"], &o["urlSlug"]]
        .into_iter().find_map(|s| s.as_str().filter(|x| !x.is_empty() && *x != "[]"));
    slug.map_or("https://store.epicgames.com/en-US/browse".to_string(), |s| format!("https://store.epicgames.com/en-US/p/{}", s.trim_end_matches("/home")))
}

/// (price, regular, percent, formatted) from a GraphQL offer, if it has a price.
fn epic_price(o: &Value) -> Option<Price> {
    let t = &o["price"]["totalPrice"];
    let div = 10f64.powi(t["currencyInfo"]["decimals"].as_i64().unwrap_or(2) as i32);
    let (fin, orig) = (t["discountPrice"].as_f64()? / div, t["originalPrice"].as_f64()? / div);
    Some(Price {
        discount: if orig > 0.0 { ((1.0 - fin / orig) * 100.0).round() as u32 } else { 0 },
        fin, initial: orig,
        formatted: t["fmtPrice"]["discountPrice"].as_str().map(str::to_string),
    })
}

pub async fn epic_sales() -> R<Vec<Deal>> {
    let q = format!(
        "query q($country:String!,$locale:String,$allowCountries:String,$onSale:Boolean,$count:Int,$start:Int,$sortBy:String,$sortDir:String,$category:String){{Catalog{{searchStore(country:$country,locale:$locale,allowCountries:$allowCountries,onSale:$onSale,count:$count,start:$start,sortBy:$sortBy,sortDir:$sortDir,category:$category){{elements{{{EPIC_OFFER_FIELDS}}}}}}}}}",
    );
    let v = epic_gql("https://store.epicgames.com/graphql", EPIC_UA_BROWSER, None, &q, json!({
        "country": "US", "allowCountries": "US", "locale": "en-US", "onSale": true, "count": 60, "start": 0,
        "sortBy": "currentPrice", "sortDir": "DESC", "category": "games/edition/base|bundles/games|editors",
    })).await?;
    let mut deals: Vec<Deal> = v["data"]["Catalog"]["searchStore"]["elements"].as_array().into_iter().flatten()
        .filter_map(|o| {
            let p = epic_price(o)?;
            if p.discount == 0 || p.fin == 0.0 { return None; } // free promos live on the Free page
            Some(Deal {
                id: format!("epic-{}", o["id"].as_str()?),
                title: o["title"].as_str()?.into(),
                shop: "Epic".into(),
                url: epic_page(o),
                image: epic_image(o),
                cut: p.discount, price: p.fin, regular: p.initial,
                app_id: None,
            })
        }).collect();
    deals.sort_by(|a, b| b.cut.cmp(&a.cut));
    Ok(deals)
}

pub async fn epic_wishlist(access: &str) -> R<Vec<WishItem>> {
    let q = format!("query w {{ Wishlist {{ wishlistItems {{ elements {{ id offerId namespace offer {{ {EPIC_OFFER_FIELDS} }} }} }} }} }}");
    // Unproven which host accepts the launcher bearer; try the launcher one first.
    let mut last = String::new();
    for (url, ua) in [
        ("https://launcher.store.epicgames.com/graphql", EPIC_UA_LAUNCHER),
        ("https://store.epicgames.com/graphql", EPIC_UA_BROWSER),
    ] {
        match epic_gql(url, ua, Some(access), &q, json!({})).await {
            Ok(v) if v["data"]["Wishlist"]["wishlistItems"]["elements"].is_array() => {
                return Ok(v["data"]["Wishlist"]["wishlistItems"]["elements"].as_array().unwrap().iter().filter_map(|e| {
                    let o = &e["offer"];
                    Some(WishItem {
                        store: "epic".into(),
                        id: e["offerId"].as_str()?.into(),
                        name: o["title"].as_str()?.into(),
                        url: epic_page(o),
                        image: epic_image(o),
                        price_info: epic_price(o),
                        app_id: None,
                        cheapest: None,
                    })
                }).collect());
            }
            Ok(_) => last = "unexpected wishlist response".into(),
            Err(e) => last = e,
        }
    }
    Err(format!("Epic wishlist unavailable ({last})"))
}
