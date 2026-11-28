//! Extend the frozen route contract without replacing historical documentation.
use serde_json::Value;

fn route_shape(path: &str) -> String {
    path.split('/')
        .map(|part| {
            if part.starts_with('{') && part.ends_with('}') {
                "{}"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub fn extend(mut historical: Value, additions: &Value) -> Result<Value, String> {
    let additions = additions
        .as_object()
        .ok_or("Native OpenAPI additions must be an object")?;
    if additions
        .keys()
        .any(|key| !matches!(key.as_str(), "paths" | "components"))
    {
        return Err("Native OpenAPI additions may only add paths and components".into());
    }
    let paths = historical["paths"]
        .as_object_mut()
        .ok_or("Historical OpenAPI paths missing")?;
    for (path, definition) in additions
        .get("paths")
        .and_then(Value::as_object)
        .ok_or("Native OpenAPI paths missing")?
    {
        if paths
            .keys()
            .any(|existing| route_shape(existing) == route_shape(path))
        {
            return Err(format!(
                "Native OpenAPI path would replace a historical route: {path}"
            ));
        }
        paths.insert(path.clone(), definition.clone());
    }
    let components = historical["components"]
        .as_object_mut()
        .ok_or("Historical OpenAPI components missing")?;
    for (category, members) in additions
        .get("components")
        .and_then(Value::as_object)
        .ok_or("Native OpenAPI components missing")?
    {
        let target = components
            .entry(category)
            .or_insert_with(|| serde_json::json!({}));
        let target = target
            .as_object_mut()
            .ok_or("Historical OpenAPI component category is invalid")?;
        for (name, definition) in members
            .as_object()
            .ok_or("Native OpenAPI components are invalid")?
        {
            if target.contains_key(name) {
                return Err(format!(
                    "Native OpenAPI component would replace a historical definition: {category}/{name}"
                ));
            }
            target.insert(name.clone(), definition.clone());
        }
    }
    Ok(historical)
}
