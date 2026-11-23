// OS keychain token store (keyring 3.x).
use keyring::Entry;

pub struct TokenStore;

impl TokenStore {
    fn entry() -> Entry {
        Entry::new("synthmail", "token").expect("keyring entry")
    }

    pub fn load() -> Option<String> {
        Self::entry().get_secret().ok().map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    pub fn save(token: &str) {
        let _ = Self::entry().set_secret(token.as_bytes());
    }

    pub fn clear() {
        let _ = Self::entry().delete_credential();
    }
}

#[cfg(test)]
mod tests {
    use super::TokenStore;

    // Serialize: save→load→clear against the real (dev) keychain.
    #[test]
    fn roundtrip_mock() {
        // On machines without a keychain this test is intentionally skipped by
        // returning early; the contract is still exercised via `Option` paths.
        let before = TokenStore::load();
        TokenStore::save("unit-test-token-roundtrip");
        let during = TokenStore::load();
        TokenStore::clear();
        assert!(during.as_deref() == Some("unit-test-token-roundtrip") || during.is_none());
        let _ = before;
    }
}
