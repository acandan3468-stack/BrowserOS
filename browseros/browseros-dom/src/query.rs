use browseros_bridge::error::BridgeResult;
use browseros_bridge::traits::{ElementPort, FramePort, LocatorEngine};

fn node_ids_from_js(frame: &dyn FramePort, js: &str) -> BridgeResult<Vec<String>> {
    let result = frame.evaluate(js, None)?;
    Ok(result
        .value
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .filter(|s| !s.is_empty())
        .collect())
}

fn resolve_port(
    engine: &dyn LocatorEngine,
    node_id: &str,
) -> BridgeResult<Option<Box<dyn ElementPort>>> {
    if node_id.is_empty() {
        return Ok(None);
    }
    let sel = format!("[cdp_node_id=\"{node_id}\"]");
    engine.query_selector(&sel)
}

pub fn query_by_text_on_frame(
    frame: &dyn FramePort,
    _engine: &dyn LocatorEngine,
    text: &str,
    exact: bool,
) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
    let escaped = text.replace('\'', "\\'");
    let expr = if exact {
        format!("el => el.textContent?.trim() === '{escaped}'")
    } else {
        format!("el => el.textContent?.includes('{escaped}')")
    };
    let js = format!(
        r#"(function() {{ return Array.from(document.querySelectorAll('*')).filter({expr}).map(el => el.cdp_node_id || '').filter(Boolean); }})()"#
    );
    let ids = node_ids_from_js(frame, &js)?;
    let mut ports = Vec::new();
    for id in ids {
        if let Ok(Some(port)) = resolve_port(_engine, &id) {
            ports.push(port);
        }
    }
    Ok(ports)
}

pub fn query_xpath_on_frame(
    frame: &dyn FramePort,
    engine: &dyn LocatorEngine,
    expr: &str,
) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
    let escaped = expr.replace('\'', "\\'");
    let js = format!(
        r#"(function() {{ const results = []; const xpathResult = document.evaluate('{escaped}', document, null, XPathResult.ORDERED_NODE_SNAPSHOT_TYPE, null); for (let i = 0; i < xpathResult.snapshotLength; i++) {{ const node = xpathResult.snapshotItem(i); if (node?.cdp_node_id) results.push(node.cdp_node_id); }} return results; }})()"#
    );
    let ids = node_ids_from_js(frame, &js)?;
    let mut ports = Vec::new();
    for id in ids {
        if let Ok(Some(port)) = resolve_port(engine, &id) {
            ports.push(port);
        }
    }
    Ok(ports)
}

pub fn sibling_by_offset(
    port: &dyn ElementPort,
    engine: &dyn LocatorEngine,
    offset: i32,
) -> BridgeResult<Option<Box<dyn ElementPort>>> {
    let frame = port.owning_frame();
    let sign = if offset > 0 {
        "nextElementSibling"
    } else {
        "previousElementSibling"
    };
    let id = port.id().get();
    let js = format!(
        r#"(function() {{ const el = document.querySelector('[cdp_node_id="{id}"]'); if (!el) return ''; const sib = el.{sign}; return sib ? (sib.cdp_node_id || '') : ''; }})()"#
    );
    let result = frame.evaluate(&js, None)?;
    let s = result
        .value
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_default();
    resolve_port(engine, &s)
}

pub fn closest_by_traversal(
    port: &dyn ElementPort,
    engine: &dyn LocatorEngine,
    selector: &str,
) -> BridgeResult<Option<Box<dyn ElementPort>>> {
    let frame = port.owning_frame();
    let id = port.id().get();
    let sel = selector.replace('\'', "\\'");
    let js = format!(
        r#"(function() {{ const el = document.querySelector('[cdp_node_id="{id}"]'); if (!el) return ''; const c = el.closest('{sel}'); return c ? (c.cdp_node_id || '') : ''; }})()"#
    );
    let result = frame.evaluate(&js, None)?;
    let s = result
        .value
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_default();
    resolve_port(engine, &s)
}

pub fn find_parent(
    port: &dyn ElementPort,
    engine: &dyn LocatorEngine,
) -> BridgeResult<Option<Box<dyn ElementPort>>> {
    let frame = port.owning_frame();
    let id = port.id().get();
    let js = format!(
        r#"(function() {{ const el = document.querySelector('[cdp_node_id="{id}"]'); if (!el || !el.parentElement) return ''; const p = el.parentElement; return p.cdp_node_id || ''; }})()"#
    );
    let result = frame.evaluate(&js, None)?;
    let s = result
        .value
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_default();
    resolve_port(engine, &s)
}

pub fn children_of(
    port: &dyn ElementPort,
    engine: &dyn LocatorEngine,
) -> BridgeResult<Vec<Box<dyn ElementPort>>> {
    let frame = port.owning_frame();
    let id = port.id().get();
    let js = format!(
        r#"(function() {{ const el = document.querySelector('[cdp_node_id="{id}"]'); if (!el) return []; return Array.from(el.children).map(c => c.cdp_node_id || '').filter(Boolean); }})()"#
    );
    let ids = node_ids_from_js(&*frame, &js)?;
    let mut ports = Vec::new();
    for id_str in ids {
        if let Ok(Some(port)) = resolve_port(engine, &id_str) {
            ports.push(port);
        }
    }
    Ok(ports)
}
