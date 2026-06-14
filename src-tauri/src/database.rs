use rusqlite::{params, Connection};
use std::path::Path;
use crate::ClipboardItem;
use crate::security::Sensitivity;
use crate::config::PersistLevel;

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
            timestamp INTEGER NOT NULL,
            thumbnail TEXT
        )",
        [],
    ).map_err(|e| e.to_string())?;

    // Try to run migration for existing databases
    let _ = conn.execute("ALTER TABLE history ADD COLUMN thumbnail TEXT", []);

    Ok(())
}

pub fn save_item(db_path: &Path, item: &ClipboardItem, level: PersistLevel) -> Result<(), String> {
    let should_save = match level {
        PersistLevel::None => item.sensitivity == Sensitivity::None,
        PersistLevel::Sensitive => item.sensitivity == Sensitivity::None || item.sensitivity == Sensitivity::Personal,
        PersistLevel::All => true,
    };

    if !should_save {
        return Ok(());
    }

    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO history (id, raw_content, display_content, content_type, sensitivity, timestamp, thumbnail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            item.id,
            item.raw_content,
            item.display_content,
            item.content_type,
            format!("{:?}", item.sensitivity),
            item.timestamp,
            item.thumbnail
        ],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_history(db_path: &Path) -> Result<Vec<ClipboardItem>, String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, raw_content, display_content, content_type, sensitivity, timestamp, thumbnail
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
            thumbnail: row.get(6)?,
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

pub fn delete_item(db_path: &Path, id: &str) -> Result<(), String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM history WHERE id = ?1",
        params![id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn clear_all(db_path: &Path) -> Result<(), String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM history",
        [],
    ).map_err(|e| e.to_string())?;
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
            thumbnail: None,
        }
    }

    #[test]
    fn test_db_persistence_rules() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("test_history_{}.db", uuid::Uuid::new_v4()));

        assert!(init_db(&db_path).is_ok());

        // 1. None level - only None sensitivity persists
        let item_none = get_dummy_item("id_none", Sensitivity::None, 1000);
        let item_personal = get_dummy_item("id_personal", Sensitivity::Personal, 1001);
        let item_secret = get_dummy_item("id_secret", Sensitivity::Secret, 1002);
        
        assert!(save_item(&db_path, &item_none, PersistLevel::None).is_ok());
        assert!(save_item(&db_path, &item_personal, PersistLevel::None).is_ok());
        assert!(save_item(&db_path, &item_secret, PersistLevel::None).is_ok());
        
        let history = load_history(&db_path).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, "id_none");

        // 2. Sensitive level - None and Personal persist, Secret and Credential do not
        let item_cred = get_dummy_item("id_cred", Sensitivity::Credential, 1003);
        assert!(save_item(&db_path, &item_personal, PersistLevel::Sensitive).is_ok());
        assert!(save_item(&db_path, &item_cred, PersistLevel::Sensitive).is_ok());
        assert!(save_item(&db_path, &item_secret, PersistLevel::Sensitive).is_ok());

        let history = load_history(&db_path).unwrap();
        // None (from test 1), Personal
        assert_eq!(history.len(), 2);
        assert!(!history.iter().any(|x| x.id == "id_secret" || x.id == "id_cred"));

        // 3. All level - everything persists including secrets
        assert!(save_item(&db_path, &item_cred, PersistLevel::All).is_ok());
        assert!(save_item(&db_path, &item_secret, PersistLevel::All).is_ok());
        let history = load_history(&db_path).unwrap();
        assert_eq!(history.len(), 4);
        assert!(history.iter().any(|x| x.id == "id_secret"));
        assert!(history.iter().any(|x| x.id == "id_cred"));

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

        assert!(save_item(&db_path, &old_cred, PersistLevel::All).is_ok());
        assert!(save_item(&db_path, &new_cred, PersistLevel::All).is_ok());
        assert!(save_item(&db_path, &old_none, PersistLevel::All).is_ok());

        assert!(run_cleanup(&db_path).is_ok());

        let history = load_history(&db_path).unwrap();
        // should have new_cred and old_none, but old_cred should be deleted
        assert_eq!(history.len(), 2);
        assert!(history.iter().any(|x| x.id == "new_cred"));
        assert!(history.iter().any(|x| x.id == "old_none"));
        assert!(!history.iter().any(|x| x.id == "old_cred"));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_db_delete_item() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("test_history_delete_{}.db", uuid::Uuid::new_v4()));

        assert!(init_db(&db_path).is_ok());

        let item = get_dummy_item("to_delete", Sensitivity::None, 1000);
        assert!(save_item(&db_path, &item, PersistLevel::All).is_ok());

        let history = load_history(&db_path).unwrap();
        assert_eq!(history.len(), 1);

        assert!(delete_item(&db_path, "to_delete").is_ok());

        let history = load_history(&db_path).unwrap();
        assert!(history.is_empty());

        let _ = std::fs::remove_file(&db_path);
    }
}
