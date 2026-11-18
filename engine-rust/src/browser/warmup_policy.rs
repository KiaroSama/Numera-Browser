use crate::{
    conversion::{truth, value_text},
    decimal::general_number,
    storage::repo::RepoError,
};
use serde_json::Value;

pub fn sites(preset: Option<&str>) -> Vec<String> {
    let values: &[&str] = match preset.unwrap_or("general").to_lowercase().as_str() {
        "ads" => &[
            "https://www.youtube.com/",
            "https://www.forbes.com/",
            "https://www.cnn.com/",
            "https://www.espn.com/",
            "https://www.buzzfeed.com/",
            "https://www.weather.com/",
            "https://www.accuweather.com/",
        ],
        "ecommerce" => &[
            "https://www.amazon.com/",
            "https://www.ebay.com/",
            "https://www.etsy.com/",
            "https://www.walmart.com/",
            "https://www.bestbuy.com/",
            "https://www.aliexpress.com/",
            "https://www.target.com/",
        ],
        "crypto" => &[
            "https://www.coingecko.com/",
            "https://coinmarketcap.com/",
            "https://www.tradingview.com/",
            "https://www.binance.com/en",
            "https://www.reddit.com/r/CryptoCurrency/",
            "https://cointelegraph.com/",
            "https://decrypt.co/",
        ],
        "social" => &[
            "https://www.reddit.com/r/popular/",
            "https://www.pinterest.com/",
            "https://www.quora.com/",
            "https://www.tumblr.com/explore",
            "https://medium.com/",
            "https://www.youtube.com/",
        ],
        "news" => &[
            "https://www.bbc.com/news",
            "https://www.reuters.com/",
            "https://apnews.com/",
            "https://www.theguardian.com/",
            "https://www.cnbc.com/",
            "https://news.ycombinator.com/",
        ],
        _ => &[
            "https://en.wikipedia.org/wiki/Special:Random",
            "https://www.bbc.com/news",
            "https://news.ycombinator.com/",
            "https://weather.com/",
            "https://www.reddit.com/r/popular/",
            "https://stackoverflow.com/questions",
            "https://www.imdb.com/chart/top/",
        ],
    };
    values.iter().map(|value| (*value).into()).collect()
}

pub fn validate(
    minutes: &Value,
    preset: Option<&Value>,
    urls: Option<&Value>,
) -> Result<f64, RepoError> {
    let fail = |message: String| RepoError::Message(message);
    let value = match minutes {
        Value::Bool(value) => Some(f64::from(u8::from(*value))),
        Value::Number(value) => value.as_f64(),
        Value::String(value) => super::cookie_expiry::numeric_text(value),
        _ => None,
    }
    .ok_or_else(|| {
        fail(format!(
            "minutes must be a number, got {}",
            crate::validation::repr(minutes)
        ))
    })?;
    if !value.is_finite() {
        return Err(fail("minutes must be a finite number".into()));
    }
    if value <= 0.0 {
        return Err(fail(format!(
            "minutes must be greater than zero, got {}",
            general_number(value)
        )));
    }
    if value > 480.0 {
        return Err(fail(format!(
            "minutes must be at most 480 (8h), got {}",
            general_number(value)
        )));
    }
    if let Some(preset) = preset.filter(|value| !value.is_null()) {
        let name = value_text(preset).to_lowercase();
        if !["general", "ads", "ecommerce", "crypto", "social", "news"].contains(&name.as_str()) {
            return Err(fail(format!(
                "unknown preset {}. Known: ads, crypto, ecommerce, general, news, social",
                crate::validation::repr(preset)
            )));
        }
    }
    if let Some(urls) = urls.filter(|value| !value.is_null()) {
        let urls = urls
            .as_array()
            .ok_or_else(|| fail("sites must be a list of URLs".into()))?;
        for url in urls {
            let text = value_text(url);
            if !text.trim().starts_with("http://") && !text.trim().starts_with("https://") {
                return Err(fail(format!(
                    "{} is not an http(s) URL",
                    crate::validation::repr(url)
                )));
            }
        }
    }
    Ok(value)
}

pub fn selected(preset: Option<&Value>, urls: Option<&Value>) -> Vec<String> {
    if let Some(urls) = urls.filter(|value| truth(value)).and_then(Value::as_array) {
        urls.iter().map(value_text).collect()
    } else {
        sites(preset.and_then(Value::as_str))
    }
}
