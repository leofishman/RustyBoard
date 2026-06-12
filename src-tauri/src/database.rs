use rusqlite::{params, Connection};
use std::path::Path;
use crate::ClipboardItem;
use crate::security::Sensitivity;

pub fn init_db(db_path: &Path) -> Result<(), String> {
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS history (
            id TEXT PRIMARY KEY,
            raw_content TEXT NOT NULL,
            display_content TEXT NOT NULL,
            content_type TEXT NOT NULL,
            sensitivity TEXT NOT NULL,
            timestamp INTEGER NOT NULL
        )",
        [],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn save_item(db_path: &Path, item: &ClipboardItem, persist_sensitive: bool) -> Result<(), String> {
    // SECURITY: Never persist Secret items to disk
    if item.sensitivity == Sensitivity::Secret {
        return Ok(());
    }

    // SECURITY: If persist_sensitive is false, only allow Sensitivity::None
    if !persist_sensitive && item.sensitivity != Sensitivity::None {
        return Ok(());
    }

    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO history (id, raw_content, display_content, content_type, sensitivity, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            item.id,
            item.raw_content,
            item.display_content,
            item.content_type,
            format!("{:?}", item.sensitivity),
            item.timestamp
        ],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_history(db_path: &Path) -> Result<Vec<ClipboardItem>, String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, raw_content, display_content, content_type, sensitivity, timestamp
         FROM history
         ORDER BY timestamp DESC
         LIMIT 100"
    ).map_err(|e| e.to_string())?;

    let rows = stmt.query_map([], |row| {
        let sens_str: String = row.get(4)?;
        let sensitivity = match sens_str.as_str() {
            "Personal" => Sensitivity::Personal,
            "Credential" => Sensitivity::Credential,
            "Secret" => Sensitivity::Secret,
            _ => Sensitivity::None,
        };

        Ok(ClipboardItem {
            id: row.get(0)?,
            raw_content: row.get(1)?,
            display_content: row.get(2)?,
            content_type: row.get(3)?,
            sensitivity,
            timestamp: row.get(5)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut items = Vec::new();
    for row in rows {
        if let Ok(item) = row {
            items.push(item);
        }
    }
    Ok(items)
}

pub fn run_cleanup(db_path: &Path) -> Result<(), String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;

    // TTL for Credential items: 2 hours (7200 seconds)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cutoff = now - 7200;

    // Delete credential entries older than 2 hours
    let _ = conn.execute(
        "DELETE FROM history WHERE sensitivity = 'Credential' AND timestamp < ?1",
        params![cutoff],
    );

    // Enforce history size limit of 100 items
    let _ = conn.execute(
        "DELETE FROM history WHERE id NOT IN (
            SELECT id FROM history ORDER BY timestamp DESC LIMIT 100
        )",
        [],
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_dummy_item(id: &str, sensitivity: Sensitivity, timestamp: u64) -> ClipboardItem {
        ClipboardItem {
            id: id.to_string(),
            raw_content: "raw".to_string(),
            display_content: "display".to_string(),
            content_type: "text".to_string(),
            sensitivity,
            timestamp,
        }
    }

    #[test]
    fn test_db_persistence_rules() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("test_history_{}.db", uuid::Uuid::new_v4()));

        assert!(init_db(&db_path).is_ok());

        // 1. None sensitivity item - should always persist
        let item_none = get_dummy_item("id_none", Sensitivity::None, 1000);
        assert!(save_item(&db_path, &item_none, false).is_ok());
        let history = load_history(&db_path).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, "id_none");

        // 2. Secret item - should NEVER persist (regardless of flag)
        let item_secret = get_dummy_item("id_secret", Sensitivity::Secret, 1002);
        assert!(save_item(&db_path, &item_secret, true).is_ok());
        let history = load_history(&db_path).unwrap();
        assert_eq!(history.len(), 1); // still only 1 item

        // 3. Credential item with persist_sensitive = false - should not persist
        let item_cred = get_dummy_item("id_cred_no", Sensitivity::Credential, 1003);
        assert!(save_item(&db_path, &item_cred, false).is_ok());
        let history = load_history(&db_path).unwrap();
        assert_eq!(history.len(), 1); // still only 1 item

        // 4. Credential item with persist_sensitive = true - should persist
        let item_cred_yes = get_dummy_item("id_cred_yes", Sensitivity::Credential, 1004);
        assert!(save_item(&db_path, &item_cred_yes, true).is_ok());
        let history = load_history(&db_path).unwrap();
        assert_eq!(history.len(), 2);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_db_cleanup() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("test_history_cleanup_{}.db", uuid::Uuid::new_v4()));

        assert!(init_db(&db_path).is_ok());

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Expired credential (older than 2 hours)
        let old_cred = get_dummy_item("old_cred", Sensitivity::Credential, now - 8000);
        // Fresh credential (10 minutes old)
        let new_cred = get_dummy_item("new_cred", Sensitivity::Credential, now - 600);
        // Regular item (older than 2 hours)
        let old_none = get_dummy_item("old_none", Sensitivity::None, now - 8000);

        assert!(save_item(&db_path, &old_cred, true).is_ok());
        assert!(save_item(&db_path, &new_cred, true).is_ok());
        assert!(save_item(&db_path, &old_none, true).is_ok());

        assert!(run_cleanup(&db_path).is_ok());

        let history = load_history(&db_path).unwrap();
        // should have new_cred and old_none, but old_cred should be deleted
        assert_eq!(history.len(), 2);
        assert!(history.iter().any(|x| x.id == "new_cred"));
        assert!(history.iter().any(|x| x.id == "old_none"));
        assert!(!history.iter().any(|x| x.id == "old_cred"));

        let _ = std::fs::remove_file(&db_path);
    }
}
