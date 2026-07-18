use crate::connection::CdpConnection;
use crate::error::{CdpError, CdpResult};
use crate::protocol::TargetInfo;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct CdpSession {
    connection: Arc<CdpConnection>,
    session_id: String,
    pub target_id: String,
}

impl CdpSession {
    pub fn new(connection: Arc<CdpConnection>, target_id: &str, session_id: &str) -> Self {
        Self {
            connection,
            session_id: session_id.to_string(),
            target_id: target_id.to_string(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    pub fn connection(&self) -> &Arc<CdpConnection> {
        &self.connection
    }

    pub fn send<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> CdpResult<T> {
        self.connection
            .send_with_session(method, params, &self.session_id)
    }

    pub fn on_event<F>(&self, method: &str, callback: F)
    where
        F: Fn(&str, &Value) + Send + Sync + 'static,
    {
        self.connection.on_event(method, callback);
    }

    pub fn detach(&self) -> CdpResult<()> {
        self.connection.send::<Value>(
            "Target.detachFromTarget",
            Some(serde_json::json!({
                "sessionId": self.session_id,
                "targetId": self.target_id,
            })),
        )?;
        Ok(())
    }
}

pub struct TargetManager {
    connection: Arc<CdpConnection>,
}

impl TargetManager {
    pub fn new(connection: Arc<CdpConnection>) -> Self {
        Self { connection }
    }

    pub fn list_targets(&self) -> CdpResult<Vec<TargetInfo>> {
        let result: Value = self.connection.send("Target.getTargets", None)?;
        let target_infos = result["targetInfos"]
            .as_array()
            .ok_or_else(|| CdpError::Serialization("Missing targetInfos".into()))?;

        target_infos
            .iter()
            .map(|t| {
                Ok(TargetInfo {
                    target_id: t["targetId"]
                        .as_str()
                        .ok_or_else(|| CdpError::Serialization("Missing targetId".into()))?
                        .to_string(),
                    target_type: t["type"]
                        .as_str()
                        .ok_or_else(|| CdpError::Serialization("Missing type".into()))?
                        .to_string(),
                    title: t["title"].as_str().unwrap_or("").to_string(),
                    url: t["url"].as_str().unwrap_or("").to_string(),
                    attached: t["attached"].as_bool().unwrap_or(false),
                    opener_id: t["openerId"].as_str().map(|s| s.to_string()),
                    browser_context_id: t["browserContextId"].as_str().map(|s| s.to_string()),
                })
            })
            .collect()
    }

    pub fn create_target(&self, url: &str) -> CdpResult<CdpSession> {
        let result: Value = self.connection.send(
            "Target.createTarget",
            Some(serde_json::json!({
                "url": url,
            })),
        )?;
        let target_id = result["targetId"]
            .as_str()
            .ok_or_else(|| CdpError::Serialization("Missing targetId".into()))?
            .to_string();
        self.attach_to_target(&target_id)
    }

    pub fn attach_to_target(&self, target_id: &str) -> CdpResult<CdpSession> {
        let result: Value = self.connection.send(
            "Target.attachToTarget",
            Some(serde_json::json!({
                "targetId": target_id,
                "flatten": true,
            })),
        )?;
        let session_id = result["sessionId"]
            .as_str()
            .ok_or_else(|| CdpError::Serialization("Missing sessionId".into()))?
            .to_string();
        Ok(CdpSession::new(
            self.connection.clone(),
            target_id,
            &session_id,
        ))
    }

    pub fn close_target(&self, target_id: &str) -> CdpResult<bool> {
        let result: Value = self.connection.send(
            "Target.closeTarget",
            Some(serde_json::json!({
                "targetId": target_id,
            })),
        )?;
        Ok(result["success"].as_bool().unwrap_or(false))
    }

    pub fn activate_target(&self, target_id: &str) -> CdpResult<()> {
        self.connection.send::<Value>(
            "Target.activateTarget",
            Some(serde_json::json!({
                "targetId": target_id,
            })),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CdpConfig;
    use crate::connection::CdpConnection;
    use crate::transport::mock::MockTransport;

    fn make_connection(responses: Vec<String>) -> (Arc<CdpConnection>, MockTransport) {
        let transport = MockTransport::with_responses(responses);
        let mut conn = CdpConnection::new(Box::new(transport), CdpConfig::default());
        conn.connect("ws://localhost:9222").unwrap();
        (Arc::new(conn), MockTransport::new())
    }

    #[test]
    fn test_cdp_session_new() {
        let (conn, _) = make_connection(vec![]);
        let session = CdpSession::new(conn, "target-1", "session-1");
        assert_eq!(session.session_id(), "session-1");
        assert_eq!(session.target_id(), "target-1");
    }

    #[test]
    fn test_cdp_session_connection() {
        let (conn, _) = make_connection(vec![]);
        let session = CdpSession::new(conn.clone(), "t1", "s1");
        assert!(Arc::ptr_eq(session.connection(), &conn));
    }

    #[test]
    fn test_target_manager_list() {
        let json = r#"{"id":1,"result":{"targetInfos":[{"targetId":"t1","type":"page","title":"Test","url":"about:blank","attached":false}]}}"#;
        let (conn, _) = make_connection(vec![json.into()]);
        let mgr = TargetManager::new(conn);
        let targets = mgr.list_targets().unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_id, "t1");
        assert_eq!(targets[0].target_type, "page");
    }

    #[test]
    fn test_target_manager_create() {
        let json = r#"{"id":1,"result":{"targetId":"t2"}}"#;
        let json2 = r#"{"id":2,"result":{"sessionId":"s2"}}"#;
        let (conn, _) = make_connection(vec![json.into(), json2.into()]);
        let mgr = TargetManager::new(conn);
        let session = mgr.create_target("https://example.com").unwrap();
        assert_eq!(session.target_id(), "t2");
        assert_eq!(session.session_id(), "s2");
    }

    #[test]
    fn test_target_manager_close() {
        let json = r#"{"id":1,"result":{"success":true}}"#;
        let (conn, _) = make_connection(vec![json.into()]);
        let mgr = TargetManager::new(conn);
        let result = mgr.close_target("t1").unwrap();
        assert!(result);
    }

    #[test]
    fn test_target_manager_activate() {
        let json = r#"{"id":1,"result":{}}"#;
        let (conn, _) = make_connection(vec![json.into()]);
        let mgr = TargetManager::new(conn);
        mgr.activate_target("t1").unwrap();
    }

    #[test]
    fn test_session_send_reads_response() {
        let json = r#"{"id":1,"result":{"value":42}}"#;
        let (conn, _) = make_connection(vec![json.into()]);
        let session = CdpSession::new(conn, "t1", "s1");
        let result: Value = session
            .send(
                "Runtime.evaluate",
                Some(serde_json::json!({"expression":"42"})),
            )
            .unwrap();
        assert_eq!(result["value"], 42);
    }

    #[test]
    fn test_session_detach() {
        let json = r#"{"id":1,"result":{}}"#;
        let (conn, _) = make_connection(vec![json.into()]);
        let session = CdpSession::new(conn, "t1", "s1");
        assert!(session.detach().is_ok());
    }

    #[test]
    fn test_session_on_event() {
        let (conn, _) = make_connection(vec![]);
        let session = CdpSession::new(conn, "t1", "s1");
        let called = std::sync::atomic::AtomicBool::new(false);
        session.on_event("Page.load", move |_, _| {
            called.store(true, std::sync::atomic::Ordering::SeqCst);
        });
    }

    #[test]
    fn test_target_manager_create_fails_without_target_id() {
        let json = r#"{"id":1,"result":{}}"#;
        let (conn, _) = make_connection(vec![json.into()]);
        let mgr = TargetManager::new(conn);
        let result = mgr.create_target("about:blank");
        assert!(result.is_err());
    }

    #[test]
    fn test_session_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CdpSession>();
        assert_send_sync::<TargetManager>();
    }
}
