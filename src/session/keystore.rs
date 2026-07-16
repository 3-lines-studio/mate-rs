use std::collections::HashMap;

pub struct KeyStore {
    path: String,
    data: HashMap<String, String>,
}

impl KeyStore {
    pub fn new(path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let data = match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => HashMap::new(),
        };
        Ok(Self {
            path: path.to_string(),
            data,
        })
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.data.insert(key.to_string(), value.to_string());
        self.save()
    }

    fn save(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let json = serde_json::to_string(&self.data)?;
        let tmp_path = format!("{}.tmp", self.path);
        std::fs::write(&tmp_path, &json)?;
        std::fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("keystore.json");
        let ks = KeyStore::new(&path.to_string_lossy()).unwrap();
        assert!(ks.get("t1").is_none());
    }

    #[test]
    fn test_set_and_get() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("keystore.json");
        let mut ks = KeyStore::new(&path.to_string_lossy()).unwrap();
        ks.set("thread-1", "session-a").unwrap();
        assert_eq!(ks.get("thread-1").unwrap(), "session-a");
    }

    #[test]
    fn test_loads_valid_map_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("keystore.json");
        std::fs::write(&path, r#"{"t1":"s1"}"#).unwrap();
        let ks = KeyStore::new(&path.to_string_lossy()).unwrap();
        assert_eq!(ks.get("t1").unwrap(), "s1");
    }

    #[test]
    fn test_persistence() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("keystore.json");
        let mut ks1 = KeyStore::new(&path.to_string_lossy()).unwrap();
        ks1.set("t1", "s1").unwrap();
        drop(ks1);

        let ks2 = KeyStore::new(&path.to_string_lossy()).unwrap();
        assert_eq!(ks2.get("t1").unwrap(), "s1");
    }

    #[test]
    fn test_corrupt_file_ignored() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("keystore.json");
        std::fs::write(&path, "not-json").unwrap();
        let ks = KeyStore::new(&path.to_string_lossy()).unwrap();
        assert!(ks.get("t1").is_none());
    }

    #[test]
    fn test_overwrite() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("keystore.json");
        let mut ks = KeyStore::new(&path.to_string_lossy()).unwrap();
        ks.set("t1", "s1").unwrap();
        ks.set("t1", "s2").unwrap();
        assert_eq!(ks.get("t1").unwrap(), "s2");
    }
}
