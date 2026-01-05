//! Tests for the Selector trait and FZF selection workflow

use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;

use rsenv::application::services::EnvironmentService;
use rsenv::infrastructure::traits::{RealFileSystem, SelectionItem, Selector};

/// Mock selector that returns a predetermined selection
struct MockSelector {
    selection_index: Option<usize>,
}

impl MockSelector {
    fn new(selection_index: Option<usize>) -> Self {
        Self { selection_index }
    }
}

impl Selector for MockSelector {
    fn select_one(
        &self,
        items: &[SelectionItem],
        _prompt: &str,
    ) -> Result<Option<SelectionItem>, String> {
        match self.selection_index {
            Some(idx) if idx < items.len() => Ok(Some(items[idx].clone())),
            Some(_) => Err("Index out of bounds".to_string()),
            None => Ok(None), // User cancelled
        }
    }
}

/// Helper to create temp env files for testing
fn create_env_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).expect("write env file");
    path
}

#[test]
fn given_directory_with_env_files_when_selecting_then_returns_correct_items() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();
    let _base = create_env_file(&temp, "base.env", "export BASE=value\n");
    let _local = create_env_file(
        &temp,
        "local.env",
        "# rsenv: base.env\nexport LOCAL=local_value\n",
    );

    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);
    let selector = MockSelector::new(Some(0)); // Select first item

    // Act - get items for selection
    let hierarchy = service.get_hierarchy(temp.path()).unwrap();
    let items: Vec<SelectionItem> = hierarchy
        .files
        .iter()
        .map(|f| SelectionItem {
            display: f.path.file_name().unwrap().to_string_lossy().to_string(),
            value: f.path.to_string_lossy().to_string(),
        })
        .collect();

    let selected = selector.select_one(&items, "Select environment:").unwrap();

    // Assert
    assert!(selected.is_some());
    let selected = selected.unwrap();
    assert!(selected.display.ends_with(".env"));
}

#[test]
fn given_user_cancels_selection_when_selecting_then_returns_none() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();
    let _base = create_env_file(&temp, "base.env", "export BASE=value\n");

    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);
    let selector = MockSelector::new(None); // User cancels

    // Act
    let hierarchy = service.get_hierarchy(temp.path()).unwrap();
    let items: Vec<SelectionItem> = hierarchy
        .files
        .iter()
        .map(|f| SelectionItem {
            display: f.path.file_name().unwrap().to_string_lossy().to_string(),
            value: f.path.to_string_lossy().to_string(),
        })
        .collect();

    let selected = selector.select_one(&items, "Select environment:").unwrap();

    // Assert
    assert!(selected.is_none());
}

#[test]
fn given_selected_file_when_building_then_outputs_merged_env() {
    // Arrange - v1 format uses export prefix
    let temp = TempDir::new().unwrap();
    let _base = create_env_file(&temp, "base.env", "export BASE=from_base\n");
    let _local = create_env_file(
        &temp,
        "local.env",
        "# rsenv: base.env\nexport LOCAL=from_local\n",
    );

    let fs = Arc::new(RealFileSystem);
    let service = EnvironmentService::new(fs);

    // Act - simulate full workflow
    let hierarchy = service.get_hierarchy(temp.path()).unwrap();
    let items: Vec<SelectionItem> = hierarchy
        .files
        .iter()
        .map(|f| SelectionItem {
            display: f.path.file_name().unwrap().to_string_lossy().to_string(),
            value: f.path.to_string_lossy().to_string(),
        })
        .collect();

    // Find local.env in items
    let local_item = items
        .iter()
        .find(|i| i.display == "local.env")
        .expect("local.env should be in items");

    // Build the selected file
    let result = service.build(&PathBuf::from(&local_item.value)).unwrap();

    // Assert - should have merged variables
    assert_eq!(result.variables.get("BASE"), Some(&"from_base".to_string()));
    assert_eq!(
        result.variables.get("LOCAL"),
        Some(&"from_local".to_string())
    );
}
