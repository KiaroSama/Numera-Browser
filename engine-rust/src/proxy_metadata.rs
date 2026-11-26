//! Country metadata and the shared proxy-check entry for saved profiles.
use reqwest::Url;
use serde_json::{Value, json};
use std::io;

fn country_code(value: &str) -> Option<String> {
    const ISO: &str = concat!(
        "AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BV BW BY BZ ",
        "CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR ",
        "GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT ",
        "JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO ",
        "MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY ",
        "QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO ",
        "TR TT TV TW TZ UA UG UM US UY UZ VA VC VG VI VN VU WF WS YE YT ZA ZM ZW"
    );
    if value.len() != 2 || !value.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return None;
    }
    let code = value.to_ascii_uppercase();
    ISO.split_ascii_whitespace()
        .any(|item| item == code)
        .then_some(code)
}

fn fallback_country_name(code: &str) -> Option<&'static str> {
    Some(match code {
        "AR" => "Argentina",
        "AT" => "Austria",
        "AU" => "Australia",
        "BE" => "Belgium",
        "BR" => "Brazil",
        "CA" => "Canada",
        "CH" => "Switzerland",
        "CN" => "China",
        "DE" => "Germany",
        "DK" => "Denmark",
        "ES" => "Spain",
        "FI" => "Finland",
        "FR" => "France",
        "GB" => "United Kingdom",
        "HK" => "Hong Kong",
        "IE" => "Ireland",
        "IN" => "India",
        "IT" => "Italy",
        "JP" => "Japan",
        "KR" => "South Korea",
        "MX" => "Mexico",
        "NL" => "Netherlands",
        "NO" => "Norway",
        "NZ" => "New Zealand",
        "PL" => "Poland",
        "PT" => "Portugal",
        "SE" => "Sweden",
        "SG" => "Singapore",
        "TW" => "Taiwan",
        "UA" => "Ukraine",
        "US" => "United States",
        "ZA" => "South Africa",
        _ => return None,
    })
}

pub(crate) fn with_country_metadata(mut result: Value) -> Value {
    let code = ["country_code", "country"]
        .into_iter()
        .filter_map(|key| result[key].as_str())
        .find_map(country_code);
    let flag: String = code.as_ref().map_or_else(
        || "--".into(),
        |code| {
            code.bytes()
                .filter_map(|byte| char::from_u32(0x1F1E6 + u32::from(byte - b'A')))
                .collect()
        },
    );
    if let Some(object) = result.as_object_mut() {
        let name = object.entry("country_name").or_insert(Value::Null);
        if !name.as_str().is_some_and(|name| !name.trim().is_empty()) {
            *name = json!(code.as_deref().and_then(fallback_country_name));
        }
        object.insert("country_code".into(), json!(code));
        object.insert("country_flag".into(), json!(flag));
    }
    result
}

pub(crate) fn profile_entry(proxy: &crate::proxy::Proxy) -> io::Result<Value> {
    let server = if proxy.server.contains("://") {
        proxy.server.clone()
    } else {
        format!("http://{}", proxy.server)
    };
    let url = Url::parse(&server)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Profile proxy URL is invalid"))?;
    let kind = match url.scheme() {
        "socks" | "socks5" | "socks5h" => "SOCKS5",
        "socks4" => "SOCKS4",
        "https" => "HTTPS",
        _ => "HTTP",
    };
    let authority = server
        .split_once("://")
        .map_or("", |(_, tail)| tail)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let explicit = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed
            .split_once(']')
            .and_then(|(_, tail)| tail.strip_prefix(':'))
    } else {
        authority.rsplit_once(':').map(|(_, port)| port)
    };
    let port = explicit
        .filter(|port| !port.is_empty())
        .map(str::parse::<u16>)
        .transpose()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Profile proxy port is invalid"))?
        .unwrap_or(0);
    let host = url
        .host_str()
        .unwrap_or("")
        .trim_start_matches('[')
        .trim_end_matches(']');
    let record = proxy.to_record();
    Ok(json!({"type":kind,"host":host,"port":port,
        "user":record["username"],"pass":record["password"]}))
}
