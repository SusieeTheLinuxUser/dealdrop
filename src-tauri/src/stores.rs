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

/// reqwest's Display includes the URL. A redirect can echo a form field into that URL.
fn net_err(e: reqwest::Error) -> String { e.without_url().to_string() }

/// Drop secrets a server may copy into error_description / errorMessage (clipboard + logs).
fn redact(mut err: String, secrets: &[&str]) -> String {
    let mut secrets: Vec<&str> = secrets.iter().copied().filter(|s| s.len() >= 8).collect();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for s in secrets { err = err.replace(s, "[redacted]"); }
    err
}

fn form_secrets<'a>(pairs: &[(&str, &'a str)]) -> Vec<&'a str> {
    pairs.iter()
        .filter(|(k, _)| !matches!(*k, "grant_type" | "redirect_uri" | "client_id" | "token_type"))
        .map(|(_, v)| *v)
        .collect()
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
        .send().await.map_err(net_err)?.text().await.map_err(net_err)?;
    if !verdict.contains("is_valid:true") {
        return Err("Steam did not confirm the sign-in".into());
    }
    steam_claimed_id(&cb)
}

fn steam_claimed_id(cb: &url::Url) -> R<String> {
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
    let secrets = form_secrets(&form);
    let v: Value = client().post("https://auth.gog.com/token").form(&form)
        .send().await.map_err(|e| redact(net_err(e), &secrets))?.json().await.map_err(|e| redact(net_err(e), &secrets))?;
    parse_gog_token(&v, &secrets, now_ms())
}

fn parse_gog_token(v: &Value, secrets: &[&str], now: u64) -> R<Tokens> {
    if let Some(e) = v["error_description"].as_str().or(v["error"].as_str()) {
        return Err(redact(format!("GOG: {e}"), secrets));
    }
    Ok(Tokens {
        access_token: v["access_token"].as_str().ok_or("GOG: no access_token")?.into(),
        refresh_token: v["refresh_token"].as_str().unwrap_or_default().into(),
        expires_at: expires_at(v, now),
        name: String::new(),
        avatar: None,
    })
}

/// `expires_in` is seconds and server-controlled: saturate instead of overflowing.
fn expires_at(v: &Value, now: u64) -> u64 {
    now.saturating_add(v["expires_in"].as_u64().unwrap_or(3600).saturating_mul(1000))
}

/// A refresh response may omit the refresh token, name or avatar: keep the old ones.
fn keep_missing(mut n: Tokens, old: &Tokens) -> Tokens {
    if n.refresh_token.is_empty() { n.refresh_token = old.refresh_token.clone(); }
    if n.name.is_empty() { n.name = old.name.clone(); }
    if n.avatar.is_none() { n.avatar = old.avatar.clone(); }
    n
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
    Ok(keep_missing(gog_token(&[("grant_type", "refresh_token"), ("refresh_token", &t.refresh_token)]).await?, t))
}

/// (username, country, currency, avatar url) — falls back to US/USD.
pub async fn gog_user(access: &str) -> (String, String, String, Option<String>) {
    parse_gog_user(&send_auth("https://embed.gog.com/userData.json", access).await.unwrap_or(Value::Null))
}

fn parse_gog_user(v: &Value) -> (String, String, String, Option<String>) {
    (
        v["username"].as_str().unwrap_or("GOG user").into(),
        v["country"].as_str().filter(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_alphabetic())).unwrap_or("US").into(), // goes into a URL: letters only
        v["selectedCurrency"]["code"].as_str().filter(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_alphabetic())).unwrap_or("USD").into(),
        v["avatar"].as_str().filter(|a| a.starts_with("https://images.gog.com/")).map(|a| format!("{a}_avm.jpg")),
    )
}

async fn send_auth(url: &str, access: &str) -> R<Value> {
    let res = client().get(url).bearer_auth(access).send().await.map_err(|e| redact(net_err(e), &[access]))?;
    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status().as_u16()));
    }
    res.json().await.map_err(|e| redact(net_err(e), &[access]))
}

pub async fn gog_wishlist(access: &str) -> R<Vec<WishItem>> {
    let v = send_auth("https://embed.gog.com/user/wishlist.json", access).await?;
    let ids = parse_gog_wishlist_ids(&v);
    if ids.is_empty() { return Ok(vec![]); }
    let (_, country, currency, _) = gog_user(access).await;

    let mut out = vec![];
    for chunk in ids.chunks(50) {
        let list = chunk.join(",");
        let info_url = format!("https://api.gog.com/products?ids={list}");
        let price_url = format!("https://api.gog.com/products/prices?ids={list}&countryCode={country}&currency={currency}");
        let (info, prices) = futures::join!(get_json(&info_url), get_json(&price_url));
        let prices = prices.unwrap_or(Value::Null);
        let info = info?;
        out.extend(parse_gog_wishlist_products(&info, &prices));
    }
    Ok(out)
}

fn parse_gog_wishlist_ids(v: &Value) -> Vec<String> {
    v["wishlist"].as_object().into_iter().flatten()
        .filter(|(_, on)| **on != false && **on != 0).map(|(k, _)| k.clone()).collect()
}

fn parse_gog_wishlist_products(products: &Value, prices: &Value) -> Vec<WishItem> {
    products.as_array().into_iter().flatten().map(|p| {
        let id = p["id"].as_u64().map(|n| n.to_string()).unwrap_or_default();
        let price = prices["_embedded"]["items"].as_array().into_iter().flatten()
            .find(|i| i["_embedded"]["product"]["id"].as_u64().map(|n| n.to_string()).as_deref() == Some(&id))
            .and_then(|i| {
                let pr = &i["_embedded"]["prices"][0];
                let cents = |k: &str| pr[k].as_str()?.split_whitespace().next()?.parse::<f64>().ok().map(|c| c / 100.0);
                let (base, fin) = (cents("basePrice")?, cents("finalPrice")?);
                let currency = pr["finalPrice"].as_str().and_then(|p| p.split_whitespace().nth(1)).map(str::to_string);
                Some(Price {
                    discount: if base > 0.0 { ((1.0 - fin / base) * 100.0).round() as u32 } else { 0 },
                    fin, initial: base,
                    formatted: Some(match currency.as_deref() {
                        _ if fin == 0.0 => "Free".into(),
                        None | Some("USD") => format!("${fin:.2}"),
                        Some(code) => format!("{fin:.2} {code}"),
                    }),
                    currency,
                })
            });
        WishItem {
            store: "gog".into(),
            url: format!("https://www.gog.com/en/game/{}", p["slug"].as_str().unwrap_or_default()),
            image: p["images"]["logo2x"].as_str().map(|u| format!("https:{u}")),
            name: p["title"].as_str().unwrap_or("GOG game").into(),
            id, app_id: None, price_info: price, cheapest: None, lowest: None,
        }
    }).collect()
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
    let mut secrets = form_secrets(&form);
    secrets.push(EPIC_BASIC);
    let v: Value = client()
        .post("https://account-public-service-prod03.ol.epicgames.com/account/api/oauth/token")
        .header("Authorization", format!("basic {EPIC_BASIC}"))
        .form(&form)
        .send().await.map_err(|e| redact(net_err(e), &secrets))?.json().await.map_err(|e| redact(net_err(e), &secrets))?;
    parse_epic_token(&v, &secrets, now_ms())
}

fn parse_epic_token(v: &Value, secrets: &[&str], now: u64) -> R<Tokens> {
    if let Some(e) = v["errorMessage"].as_str().or(v["errorCode"].as_str()) {
        return Err(redact(format!("Epic: {e}"), secrets));
    }
    Ok(Tokens {
        access_token: v["access_token"].as_str().ok_or("Epic: no access_token")?.into(),
        refresh_token: v["refresh_token"].as_str().unwrap_or_default().into(),
        expires_at: expires_at(v, now),
        name: v["displayName"].as_str().unwrap_or_default().into(), // empty → keep_missing / "Epic user"
        avatar: None, // Epic exposes no public avatar
    })
}

pub async fn epic_login(app: &AppHandle) -> R<Tokens> {
    let start = "https://www.epicgames.com/id/login?redirectUrl=https%3A//www.epicgames.com/id/api/redirect%3FclientId%3D34a02cf8f4414e29b15921876da36f9a%26responseType%3Dcode";
    let cb = sign_in_window(app, "epic-login", "Sign in to Epic Games", start, Some(EPIC_JS), |u| u.starts_with("https://dealdrop.invalid/")).await?;
    let code = cb.query_pairs().find(|(k, _)| k == "code").map(|(_, v)| v.to_string()).ok_or("Epic callback had no code")?;
    let mut t = epic_token(&[("grant_type", "authorization_code"), ("code", &code)]).await?;
    if t.name.is_empty() { t.name = "Epic user".into(); }
    Ok(t)
}

pub async fn epic_refresh(t: &Tokens) -> R<Tokens> {
    Ok(keep_missing(epic_token(&[("grant_type", "refresh_token"), ("refresh_token", &t.refresh_token)]).await?, t))
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
    let res = req.send().await.map_err(|e| match bearer { Some(b) => redact(net_err(e), &[b]), None => net_err(e) })?;
    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status().as_u16()));
    }
    let v: Value = res.json().await.map_err(|e| match bearer { Some(b) => redact(net_err(e), &[b]), None => net_err(e) })?;
    if let Some(e) = v["errors"][0]["message"].as_str() {
        if v["data"].is_null() {
            return Err(match bearer { Some(b) => redact(e.to_string(), &[b]), None => e.to_string() });
        }
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
        currency: Some("USD".into()), // price(country:"US")
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
    Ok(parse_epic_sales(&v))
}

/// Discounted, non-free offers, biggest cut first.
fn parse_epic_sales(v: &Value) -> Vec<Deal> {
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
                lowest: None,
                app_id: None,
            })
        }).collect();
    deals.sort_by_key(|d| std::cmp::Reverse(d.cut));
    deals
}

/// Find an Epic product by exact title only when a user opens a CheapShark deal.
pub async fn epic_product_for_title(title: &str) -> R<Option<String>> {
    let q = format!(
        "query q($country:String!,$locale:String,$keywords:String,$count:Int,$start:Int){{Catalog{{searchStore(country:$country,locale:$locale,keywords:$keywords,count:$count,start:$start){{elements{{title productSlug urlSlug catalogNs{{mappings(pageType:\"productHome\"){{pageSlug}}}} offerMappings{{pageSlug}}}}}}}}}}"
    );
    let v = epic_gql("https://store.epicgames.com/graphql", EPIC_UA_BROWSER, None, &q, json!({
        "country": "US", "locale": "en-US", "keywords": title, "count": 60, "start": 0,
    })).await?;
    Ok(parse_epic_product(&v["data"]["Catalog"]["searchStore"]["elements"], title))
}

fn parse_epic_product(products: &Value, title: &str) -> Option<String> {
    products.as_array()?.iter().find_map(|p| {
        if !p["title"].as_str()?.trim().eq_ignore_ascii_case(title.trim()) { return None; }
        [&p["offerMappings"][0]["pageSlug"], &p["catalogNs"]["mappings"][0]["pageSlug"], &p["productSlug"], &p["urlSlug"]]
            .into_iter().find_map(|v| v.as_str().filter(|s| !s.is_empty() && *s != "[]")
                .map(|s| s.trim_end_matches("/home").to_string()))
    })
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
            Ok(v) => match parse_epic_wishlist(&v) {
                Ok(items) => return Ok(items),
                Err(e) => last = e,
            },
            Err(e) => last = e,
        }
    }
    Err(format!("Epic wishlist unavailable ({last})"))
}

fn parse_epic_wishlist(v: &Value) -> R<Vec<WishItem>> {
    let elements = v["data"]["Wishlist"]["wishlistItems"]["elements"].as_array()
        .ok_or("unexpected wishlist response")?;
    Ok(elements.iter().filter_map(|e| {
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
            lowest: None,
        })
    }).collect())
}

#[cfg(test)]
mod unit {
    use serde_json::json;

    #[test]
    fn gog_wishlist_ids_skip_off_entries() {
        let v = json!({"wishlist": {"10": true, "20": false, "30": 0, "40": 1}});
        let mut ids = super::parse_gog_wishlist_ids(&v);
        ids.sort();
        assert_eq!(ids, vec!["10".to_string(), "40".to_string()]);
        assert!(super::parse_gog_wishlist_ids(&json!({})).is_empty());
        assert!(super::parse_gog_wishlist_ids(&json!({"wishlist": []})).is_empty());
    }

    #[test]
    fn gog_wishlist_parses_products_and_prices() {
        let products = json!([
            {"id": 1454315831, "title": "The Witcher 3", "slug": "the_witcher_3_wild_hunt", "images": {"logo2x": "//images.gog.com/logo.jpg"}},
            {"id": 2, "title": "Unpriced", "slug": "unpriced"}
        ]);
        let prices = json!({"_embedded": {"items": [{"_embedded": {
            "product": {"id": 1454315831},
            "prices": [{"basePrice": "1000 USD", "finalPrice": "500 USD"}]
        }}]}});
        let v = super::parse_gog_wishlist_products(&products, &prices);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].store, "gog");
        assert_eq!(v[0].id, "1454315831");
        assert_eq!(v[0].name, "The Witcher 3");
        assert_eq!(v[0].url, "https://www.gog.com/en/game/the_witcher_3_wild_hunt");
        assert_eq!(v[0].image.as_deref(), Some("https://images.gog.com/logo.jpg"));
        assert!(v[0].app_id.is_none());
        let p = v[0].price_info.as_ref().unwrap();
        assert_eq!(p.discount, 50);
        assert_eq!(p.fin, 5.0);
        assert_eq!(p.initial, 10.0);
        assert_eq!(p.formatted.as_deref(), Some("$5.00"));
        assert_eq!(p.currency.as_deref(), Some("USD"));
        assert!(v[1].price_info.is_none());
        assert_eq!(v[1].name, "Unpriced");
        assert!(super::parse_gog_wishlist_products(&json!({}), &json!(null)).is_empty());
        let eur = json!({"_embedded": {"items": [{"_embedded": {
            "product": {"id": 1454315831},
            "prices": [{"basePrice": "1000 EUR", "finalPrice": "500 EUR"}]
        }}]}});
        let p = super::parse_gog_wishlist_products(&products, &eur)[0].price_info.clone().unwrap();
        assert_eq!(p.formatted.as_deref(), Some("5.00 EUR"));
    }

    #[test]
    fn epic_wishlist_parses_elements() {
        let v = json!({"data": {"Wishlist": {"wishlistItems": {"elements": [
            {
                "offerId": "offer1",
                "offer": {
                    "title": "Hades",
                    "productSlug": "hades/home",
                    "keyImages": [
                        {"type": "Thumbnail", "url": "https://cdn.example/thumb.jpg"},
                        {"type": "OfferImageWide", "url": "https://cdn.example/wide.jpg"}
                    ],
                    "price": {"totalPrice": {
                        "discountPrice": 500, "originalPrice": 1000,
                        "currencyInfo": {"decimals": 2},
                        "fmtPrice": {"discountPrice": "$5.00"}
                    }}
                }
            },
            {"offer": {"title": "No Id"}}
        ]}}}});
        let rows = super::parse_epic_wishlist(&v).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].store, "epic");
        assert_eq!(rows[0].id, "offer1");
        assert_eq!(rows[0].name, "Hades");
        assert_eq!(rows[0].url, "https://store.epicgames.com/en-US/p/hades");
        assert_eq!(rows[0].image.as_deref(), Some("https://cdn.example/wide.jpg"));
        let p = rows[0].price_info.as_ref().unwrap();
        assert_eq!(p.discount, 50);
        assert_eq!(p.fin, 5.0);
        assert_eq!(p.initial, 10.0);
        assert_eq!(p.formatted.as_deref(), Some("$5.00"));
        assert!(super::parse_epic_wishlist(&json!({})).is_err());
        assert!(super::parse_epic_wishlist(&json!({"data": {"Wishlist": {"wishlistItems": {"elements": []}}}})).unwrap().is_empty());
    }

    #[test]
    fn epic_deal_lookup_is_exact_and_returns_a_slug() {
        let products = json!([
            {"title":"  Alan Wake 2 ", "productSlug":"alan-wake-2/home"},
            {"title":"Alan Wake", "productSlug":"alan-wake"}
        ]);
        assert_eq!(super::parse_epic_product(&products, "alan wake 2"), Some("alan-wake-2".into()));
        assert_eq!(super::parse_epic_product(&products, "Alan"), None);
        let unsafe_slug = json!([{"title":"A Game", "productSlug":"bad slug"}]);
        assert_eq!(super::parse_epic_product(&unsafe_slug, "A Game"), Some("bad slug".into()));
    }

    #[test]
    fn gog_user_validates_country_currency_and_avatar() {
        let (name, country, currency, avatar) = super::parse_gog_user(&json!({
            "username": "Geralt", "country": "PL", "selectedCurrency": {"code": "PLN", "symbol": "zł"},
            "avatar": "https://images.gog.com/abc123"
        }));
        assert_eq!((name.as_str(), country.as_str(), currency.as_str()), ("Geralt", "PL", "PLN"));
        assert_eq!(avatar.as_deref(), Some("https://images.gog.com/abc123_avm.jpg"));
        // Anything that would go into a URL unchecked falls back
        let (name, country, currency, avatar) = super::parse_gog_user(&json!({
            "country": "P&L", "selectedCurrency": {"code": "US"}, "avatar": "https://evil.example/a"
        }));
        assert_eq!((name.as_str(), country.as_str(), currency.as_str()), ("GOG user", "US", "USD"));
        assert!(avatar.is_none());
        assert_eq!(super::parse_gog_user(&serde_json::Value::Null).1, "US");
    }

    #[test]
    fn gog_wishlist_free_and_zero_base_prices() {
        let products = json!([{"id": 7, "title": "Freebie"}, {"id": 8, "title": "Odd"}, {"title": "No Id"}]);
        let prices = json!({"_embedded": {"items": [
            {"_embedded": {"product": {"id": 7}, "prices": [{"basePrice": "999 USD", "finalPrice": "0 USD"}]}},
            {"_embedded": {"product": {"id": 8}, "prices": [{"basePrice": "0 GBP", "finalPrice": "0 GBP"}]}}
        ]}});
        let v = super::parse_gog_wishlist_products(&products, &prices);
        assert_eq!(v.len(), 3);
        let free = v[0].price_info.as_ref().unwrap();
        assert_eq!((free.discount, free.fin, free.initial), (100, 0.0, 9.99));
        assert_eq!(free.formatted.as_deref(), Some("Free"));
        let odd = v[1].price_info.as_ref().unwrap();
        assert_eq!(odd.discount, 0);
        assert_eq!(odd.currency.as_deref(), Some("GBP"));
        // No slug/image/id: still listed with empty fallbacks (not dropped)
        assert_eq!(v[1].url, "https://www.gog.com/en/game/");
        assert!(v[1].image.is_none());
        assert_eq!(v[2].id, "");
        assert!(v[2].price_info.is_none());
    }

    #[test]
    fn epic_sales_keep_paid_discounts_biggest_first() {
        let offer = |id: &str, fin: u64, orig: u64| json!({
            "title": id, "id": id, "offerMappings": [{"pageSlug": format!("{id}-page"), "pageType": "productHome"}],
            "keyImages": [{"type": "Thumbnail", "url": "https://cdn.example/t.jpg"}, {"type": "OfferImageTall", "url": "https://cdn.example/tall.jpg"}],
            "price": {"totalPrice": {"discountPrice": fin, "originalPrice": orig, "discount": orig - fin, "currencyInfo": {"decimals": 2}}}
        });
        let v = json!({"data": {"Catalog": {"searchStore": {"elements": [
            offer("small", 1800, 2000),
            offer("big", 500, 2000),
            offer("free", 0, 2000),
            offer("full", 2000, 2000),
            {"title": "No Price", "id": "np"},
            {"title": "Yen", "id": "yen", "price": {"totalPrice": {"discountPrice": 750, "originalPrice": 1500, "currencyInfo": {"decimals": 0}}}}
        ]}}}});
        let d = super::parse_epic_sales(&v);
        assert_eq!(d.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["epic-big", "epic-yen", "epic-small"]);
        assert_eq!((d[0].cut, d[0].price, d[0].regular), (75, 5.0, 20.0));
        assert_eq!(d[0].shop, "Epic");
        assert_eq!(d[0].url, "https://store.epicgames.com/en-US/p/big-page");
        assert_eq!(d[0].image.as_deref(), Some("https://cdn.example/tall.jpg"));
        // decimals: 0 → whole units
        assert_eq!((d[1].price, d[1].regular), (750.0, 1500.0));
        assert_eq!(d[1].url, "https://store.epicgames.com/en-US/browse");
        assert!(d[1].image.is_none());
        assert!(super::parse_epic_sales(&json!({"data": null})).is_empty());
    }

    const ACCESS: &str = "acc-3f9a1c7e5b2d4a6f";
    const REFRESH: &str = "ref-8e2b6d4f1a3c5e7b";
    const CODE: &str = "code-5c1e9a7b3d2f4e6a";

    #[test]
    fn gog_token_success_and_refresh_responses() {
        let v = json!({"access_token": ACCESS, "refresh_token": REFRESH, "expires_in": 3600, "user_id": "4815"});
        let t = super::parse_gog_token(&v, &[], 1_000).unwrap();
        assert_eq!((t.access_token.as_str(), t.refresh_token.as_str()), (ACCESS, REFRESH));
        assert_eq!(t.expires_at, 1_000 + 3_600_000);
        assert!(t.name.is_empty() && t.avatar.is_none()); // filled from userData.json / the old tokens

        // Refresh without a new refresh token: keep the old one, name and avatar.
        let old = super::Tokens { refresh_token: REFRESH.into(), name: "gogger".into(), avatar: Some("https://images.gog.com/a_avm.jpg".into()), ..Default::default() };
        let n = super::parse_gog_token(&json!({"access_token": "acc-new-0000000000", "expires_in": 60}), &[], 5_000).unwrap();
        let n = super::keep_missing(n, &old);
        assert_eq!(n.access_token, "acc-new-0000000000");
        assert_eq!(n.refresh_token, REFRESH);
        assert_eq!(n.name, "gogger");
        assert_eq!(n.avatar, old.avatar);
        assert_eq!(n.expires_at, 65_000);
    }

    #[test]
    fn gog_token_errors_never_carry_the_secrets() {
        let secrets = [REFRESH, CODE];
        let e = super::parse_gog_token(&json!({"error": "invalid_grant", "error_description": format!("refresh_token {REFRESH} is revoked")}), &secrets, 0).unwrap_err();
        assert!(e.starts_with("GOG: refresh_token ") && e.contains("[redacted]"), "{e}");
        assert!(!e.contains(REFRESH) && !e.contains(CODE));
        let e = super::parse_gog_token(&json!({"error": format!("bad code {CODE}")}), &secrets, 0).unwrap_err();
        assert_eq!(e, "GOG: bad code [redacted]");
        // An error wins over a token in the same body; a body without one is an error too.
        assert!(super::parse_gog_token(&json!({"error": "x", "access_token": ACCESS}), &secrets, 0).is_err());
        assert_eq!(super::parse_gog_token(&json!({"refresh_token": REFRESH}), &secrets, 0).unwrap_err(), "GOG: no access_token");
        assert!(super::parse_gog_token(&json!({"access_token": 42}), &secrets, 0).is_err());
        assert!(super::parse_gog_token(&serde_json::Value::Null, &secrets, 0).is_err());
    }

    #[test]
    fn epic_token_success_refresh_and_errors() {
        let v = json!({"access_token": ACCESS, "refresh_token": REFRESH, "expires_in": 7200, "account_id": "abc", "displayName": "EpicPerson"});
        let t = super::parse_epic_token(&v, &[], 0).unwrap();
        assert_eq!((t.access_token.as_str(), t.refresh_token.as_str(), t.name.as_str()), (ACCESS, REFRESH, "EpicPerson"));
        assert_eq!(t.expires_at, 7_200_000);
        assert!(t.avatar.is_none());

        // A refresh body without displayName / refresh_token keeps the signed-in name and token.
        let old = super::Tokens { refresh_token: REFRESH.into(), name: "EpicPerson".into(), ..Default::default() };
        let n = super::keep_missing(super::parse_epic_token(&json!({"access_token": "acc-new-0000000000"}), &[], 0).unwrap(), &old);
        assert_eq!((n.name.as_str(), n.refresh_token.as_str()), ("EpicPerson", REFRESH));
        // A renamed account takes the new name.
        let n = super::keep_missing(super::parse_epic_token(&json!({"access_token": "a", "displayName": "Renamed"}), &[], 0).unwrap(), &old);
        assert_eq!(n.name, "Renamed");

        let secrets = [REFRESH, super::EPIC_BASIC];
        let e = super::parse_epic_token(&json!({
            "errorCode": "errors.com.epicgames.account.auth_token.invalid_refresh_token",
            "errorMessage": format!("Sorry the refresh token '{REFRESH}' is invalid"),
        }), &secrets, 0).unwrap_err();
        assert_eq!(e, "Epic: Sorry the refresh token '[redacted]' is invalid");
        let e = super::parse_epic_token(&json!({"errorCode": "errors.com.epicgames.common.oauth.invalid_client", "access_token": ACCESS}), &secrets, 0).unwrap_err();
        assert_eq!(e, "Epic: errors.com.epicgames.common.oauth.invalid_client");
        let e = super::parse_epic_token(&json!({"errorMessage": format!("bad client {}", super::EPIC_BASIC)}), &secrets, 0).unwrap_err();
        assert!(!e.contains(super::EPIC_BASIC), "{e}");
        assert_eq!(super::parse_epic_token(&json!({}), &secrets, 0).unwrap_err(), "Epic: no access_token");
    }

    #[test]
    fn token_expiry_math() {
        let at = |v| super::expires_at(&v, 10_000);
        assert_eq!(at(json!({})), 10_000 + 3_600_000); // missing → 1 h
        assert_eq!(at(json!({"expires_in": "3600"})), 10_000 + 3_600_000); // not a number → 1 h
        assert_eq!(at(json!({"expires_in": -5})), 10_000 + 3_600_000);
        assert_eq!(at(json!({"expires_in": 0})), 10_000);
        assert_eq!(at(json!({"expires_in": u64::MAX})), u64::MAX); // saturates, no overflow panic
        assert_eq!(super::expires_at(&json!({"expires_in": 1}), u64::MAX - 10), u64::MAX);

        let t = |expires_at| super::Tokens { expires_at, ..Default::default() };
        assert!(t(0).stale());
        assert!(t(super::now_ms() + 30_000).stale()); // refreshed a minute early
        assert!(!t(super::now_ms() + 3_600_000).stale());
    }

    #[test]
    fn form_secrets_and_redact() {
        let form = [("client_id", "46899977096215655"), ("client_secret", "sssssssssss"), ("grant_type", "refresh_token"),
            ("refresh_token", REFRESH), ("redirect_uri", "https://embed.gog.com/on_login_success?origin=client"), ("code", CODE)];
        assert_eq!(super::form_secrets(&form), vec!["sssssssssss", REFRESH, CODE]);
        // Longest first, so a secret containing another is redacted whole; short values are left alone.
        assert_eq!(super::redact(format!("{REFRESH}xyz {CODE} abc"), &["abc", REFRESH, &format!("{REFRESH}xyz")]), format!("[redacted] {CODE} abc"));
    }

    #[test]
    fn steam_claimed_id_must_be_a_steamcommunity_id() {
        let cb = |claimed: &str| {
            let mut u = url::Url::parse("http://localhost/steamcallback").unwrap();
            u.query_pairs_mut().append_pair("openid.mode", "id_res").append_pair("openid.claimed_id", claimed);
            super::steam_claimed_id(&u)
        };
        assert_eq!(cb("https://steamcommunity.com/openid/id/76561197960287930").unwrap(), "76561197960287930");
        assert!(cb("https://evil.example/openid/id/76561197960287930").is_err());
        assert!(cb("https://steamcommunity.com/openid/id/1234").is_err());
        assert!(cb("https://steamcommunity.com/openid/id/7656119796028793x").is_err());
        assert!(super::steam_claimed_id(&url::Url::parse("http://localhost/steamcallback").unwrap()).is_err());
    }
}
