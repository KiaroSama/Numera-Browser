use crate::{devices::catalog, identity, models::Profile, validation};
use serde_json::{Value, json};
use std::io;

const RESOLVERS: [(&str, &str); 8] = [
    ("1.1.1.1", "https://cloudflare-dns.com/dns-query"),
    ("1.0.0.1", "https://cloudflare-dns.com/dns-query"),
    ("8.8.8.8", "https://dns.google/dns-query"),
    ("8.8.4.4", "https://dns.google/dns-query"),
    ("9.9.9.9", "https://dns.quad9.net/dns-query"),
    ("149.112.112.112", "https://dns.quad9.net/dns-query"),
    ("94.140.14.14", "https://dns.adguard-dns.com/dns-query"),
    ("208.67.222.222", "https://doh.opendns.com/dns-query"),
];

fn dns_args(profile: &Profile) -> io::Result<Vec<String>> {
    let dns = crate::pool::trim(&profile.dns);
    if dns.is_empty() {
        return Ok(Vec::new());
    }
    let template = if dns.starts_with("https://") {
        Some(dns)
    } else {
        RESOLVERS
            .iter()
            .find(|(ip, _)| *ip == dns)
            .map(|(_, url)| *url)
    };
    let Some(template) = template else {
        let mut keys: Vec<_> = RESOLVERS.iter().map(|(ip, _)| *ip).collect();
        keys.sort_unstable();
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} is not a DNS-over-HTTPS resolver. Chromium cannot be pointed at a plain nameserver; give a DoH URL (https://...) or one of: {}.",
                validation::repr(&Value::String(dns.into())),
                keys.join(", ")
            ),
        ));
    };
    Ok(vec![
        "--enable-features=DnsOverHttps".into(),
        format!("--dns-over-https-templates={template}"),
        "--dns-over-https-mode=secure".into(),
    ])
}

fn geolocation(profile: &Profile) -> io::Result<Option<Value>> {
    let data = &catalog().map_err(io::Error::other)?["catalog"];
    let country = profile
        .proxy
        .as_ref()
        .and_then(|proxy| proxy.country.as_deref())
        .filter(|country| !country.is_empty())
        .map(str::to_uppercase)
        .or_else(|| {
            data["LOCALE_TO_COUNTRY"][&profile.fingerprint.locale]
                .as_str()
                .map(str::to_owned)
        });
    let Some(coords) = country.and_then(|country| data["COUNTRY_GEO"][country].as_array()) else {
        return Ok(None);
    };
    Ok(Some(
        json!({"latitude":coords[0], "longitude":coords[1], "accuracy":60}),
    ))
}

/// Build SDK options from a current, already IP-resolved profile. Resource paths
/// and relay endpoints come from native services, not application-aware adapters.
pub fn build(
    profile: &Profile,
    backend: &str,
    user_data_dir: &str,
    headless: bool,
    extra_args: &[String],
    extensions: &[String],
    relay: Option<&str>,
) -> io::Result<Value> {
    if !["playwright", "patchright", "camoufox", "cloak"].contains(&backend) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unknown browser backend",
        ));
    }
    let proxy = match &profile.proxy {
        None => Value::Null,
        Some(proxy) => {
            let options = proxy.to_playwright();
            if options.get("username").is_some() || options.get("password").is_some() {
                json!({"server": relay.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Authenticated proxy requires a native relay endpoint"))?})
            } else {
                options
            }
        }
    };
    let fp = &profile.fingerprint;
    if backend == "camoufox" {
        let mut options = json!({"headless":headless, "os":if ["windows","macos","linux"].contains(&fp.os.as_str()) { &fp.os } else { "windows" }, "locale":fp.locale, "persistent_context":true, "user_data_dir":user_data_dir});
        if !proxy.is_null() {
            options["proxy"] = proxy;
            if !profile.ip_based.is_empty() {
                options["geoip"] = true.into();
            }
        }
        return Ok(options);
    }
    let mut args = if backend == "cloak" {
        vec![
            "--test-type".into(),
            "--no-first-run".into(),
            "--no-default-browser-check".into(),
        ]
    } else {
        vec![
            format!("--window-size={},{}", fp.viewport_width, fp.viewport_height),
            "--disable-blink-features=AutomationControlled".into(),
            "--no-first-run".into(),
            "--no-default-browser-check".into(),
        ]
    };
    args.push(format!(
        "--force-webrtc-ip-handling-policy={}",
        if profile.proxy.is_some() {
            "disable_non_proxied_udp"
        } else {
            "default_public_interface_only"
        }
    ));
    if !extensions.is_empty() {
        let joined = extensions.join(",");
        args.extend([
            format!("--disable-extensions-except={joined}"),
            format!("--load-extension={joined}"),
        ]);
    }
    args.extend(dns_args(profile)?);
    for key in &profile.switched_off {
        let flags: &[&str] = match key.as_str() {
            "googleServices" => &[
                "--disable-sync",
                "--disable-background-networking",
                "--disable-client-side-phishing-detection",
                "--no-pings",
                "--disable-domain-reliability",
            ],
            "systemExtensions" => &[
                "--disable-component-extensions-with-background-pages",
                "--disable-component-update",
            ],
            _ => &[],
        };
        args.extend(flags.iter().map(|flag| (*flag).into()));
    }
    args.extend(profile.launch_args.iter().cloned());
    args.extend(extra_args.iter().cloned());
    if backend == "cloak" {
        return Ok(
            json!({"user_data_dir":user_data_dir,"headless":headless,"proxy":proxy,"user_agent":fp.user_agent,"viewport":{"width":fp.viewport_width,"height":fp.viewport_height},"locale":fp.locale,"timezone":fp.timezone,"geoip":profile.proxy.is_some(),"args":args}),
        );
    }
    let mut options = json!({"user_data_dir":user_data_dir,"headless":headless,"channel":if headless { Some("chromium") } else { None },"args":args,"ignore_default_args":["--enable-automation"],"user_agent":fp.user_agent,"locale":fp.locale,"timezone_id":fp.timezone,"viewport":{"width":fp.viewport_width,"height":fp.viewport_height},"screen":{"width":fp.screen_width,"height":fp.screen_height},"is_mobile":fp.is_mobile,"device_scale_factor":if fp.device_pixel_ratio.as_f64().is_some_and(|value| value != 0.0) { Value::Number(fp.device_pixel_ratio.clone()) } else if fp.is_mobile { json!(2.0) } else { json!(1.0) },"has_touch":fp.max_touch_points>0});
    let mut headers = identity::headers(fp).map_err(io::Error::other)?["low"].clone();
    headers["accept-language"] = fp
        .languages
        .iter()
        .enumerate()
        .map(|(index, language)| {
            if index == 0 {
                language.clone()
            } else {
                format!("{language};q={:.1}", 1.0 - index as f64 * 0.1)
            }
        })
        .collect::<Vec<_>>()
        .join(",")
        .into();
    if fp.do_not_track {
        headers["dnt"] = "1".into();
    }
    options["extra_http_headers"] = headers;
    if !proxy.is_null() {
        options["proxy"] = proxy;
    }
    let mode = if fp.geolocation.is_empty() {
        "prompt".into()
    } else {
        fp.geolocation.to_lowercase()
    };
    if mode == "block" {
        options["permissions"] = json!([]);
    } else if let Some(coords) = geolocation(profile)? {
        options["geolocation"] = coords;
        if mode == "allow" {
            options["permissions"] = json!(["geolocation"]);
        }
    }
    Ok(options)
}
