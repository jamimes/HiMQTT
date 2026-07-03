use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use uuid::Uuid;

#[derive(Clone)]
pub struct AdminAuth {
    sessions: Arc<RwLock<HashMap<String, String>>>,
}

impl AdminAuth {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn create_session(&self, username: &str) -> String {
        let token = Uuid::new_v4().to_string();
        self.sessions
            .write()
            .unwrap()
            .insert(token.clone(), username.to_owned());
        token
    }

    pub fn validate(&self, token: &str) -> Option<String> {
        self.sessions.read().ok()?.get(token).cloned()
    }

    pub fn revoke(&self, token: &str) {
        if let Ok(mut sessions) = self.sessions.write() {
            sessions.remove(token);
        }
    }
}

impl Default for AdminAuth {
    fn default() -> Self {
        Self::new()
    }
}
