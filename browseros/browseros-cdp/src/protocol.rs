use crate::error::CdpResult;

#[derive(Debug, Clone)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

impl ProtocolVersion {
    pub fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    pub fn current() -> Self {
        Self { major: 1, minor: 3 }
    }

    pub fn supports_domain(&self, domain: &str) -> bool {
        match domain {
            "Page" | "Runtime" | "DOM" | "Target" | "Browser" => true,
            "Network" | "Input" | "Storage" | "CSS" => self.major >= 1,
            "Fetch" | "WebAuthn" | "Media" => self.major >= 1 && self.minor >= 2,
            "BackgroundService" | "Performance" => self.major >= 1 && self.minor >= 3,
            _ => self.major >= 1,
        }
    }
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

#[derive(Debug, Clone)]
pub struct BrowserVersion {
    pub product: String,
    pub revision: String,
    pub user_agent: String,
    pub js_version: String,
}

#[derive(Debug, Clone)]
pub struct TargetInfo {
    pub target_id: String,
    pub target_type: String,
    pub title: String,
    pub url: String,
    pub attached: bool,
    pub opener_id: Option<String>,
    pub browser_context_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProtocolDomain {
    pub name: String,
    pub version: String,
}

impl ProtocolDomain {
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
        }
    }
}

pub fn parse_protocol_version(version_str: &str) -> CdpResult<ProtocolVersion> {
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() != 2 {
        return Ok(ProtocolVersion::current());
    }
    let major = parts[0].parse::<u32>().unwrap_or(1);
    let minor = parts[1].parse::<u32>().unwrap_or(3);
    Ok(ProtocolVersion::new(major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_current() {
        let v = ProtocolVersion::current();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 3);
    }

    #[test]
    fn test_protocol_version_display() {
        let v = ProtocolVersion::new(1, 2);
        assert_eq!(v.to_string(), "1.2");
    }

    #[test]
    fn test_supports_core_domains() {
        let v = ProtocolVersion::new(1, 3);
        assert!(v.supports_domain("Page"));
        assert!(v.supports_domain("Runtime"));
        assert!(v.supports_domain("DOM"));
        assert!(v.supports_domain("Target"));
        assert!(v.supports_domain("Browser"));
    }

    #[test]
    fn test_supports_network_domain() {
        let v = ProtocolVersion::new(1, 0);
        assert!(v.supports_domain("Network"));
    }

    #[test]
    fn test_supports_advanced_domains() {
        let v_old = ProtocolVersion::new(1, 1);
        let v_new = ProtocolVersion::new(1, 3);

        assert!(!v_old.supports_domain("Fetch"));
        assert!(v_new.supports_domain("BackgroundService"));
        assert!(v_new.supports_domain("Performance"));
    }

    #[test]
    fn test_unknown_domain() {
        let v = ProtocolVersion::new(1, 3);
        assert!(v.supports_domain("UnknownDomain"));
    }

    #[test]
    fn test_parse_valid() {
        let v = parse_protocol_version("1.2").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
    }

    #[test]
    fn test_parse_invalid_fallback() {
        let v = parse_protocol_version("invalid").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 3);
    }

    #[test]
    fn test_browser_version_struct() {
        let bv = BrowserVersion {
            product: "Chrome/120.0.0.0".into(),
            revision: "@abc123".into(),
            user_agent: "Mozilla/5.0 ...".into(),
            js_version: "12.0.0".into(),
        };
        assert!(bv.product.contains("Chrome"));
    }

    #[test]
    fn test_target_info() {
        let ti = TargetInfo {
            target_id: "abc123".into(),
            target_type: "page".into(),
            title: "Test".into(),
            url: "https://example.com".into(),
            attached: false,
            opener_id: None,
            browser_context_id: None,
        };
        assert_eq!(ti.target_type, "page");
        assert!(!ti.attached);
    }

    #[test]
    fn test_protocol_version_new() {
        let v = ProtocolVersion::new(1, 0);
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
    }

    #[test]
    fn test_protocol_domain() {
        let d = ProtocolDomain::new("Page", "1.3");
        assert_eq!(d.name, "Page");
        assert_eq!(d.version, "1.3");
    }

    #[test]
    fn test_protocol_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ProtocolVersion>();
        assert_send_sync::<BrowserVersion>();
        assert_send_sync::<TargetInfo>();
    }
}
