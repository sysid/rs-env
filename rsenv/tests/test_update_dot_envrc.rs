use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use fs_extra::{copy_items, dir};
use rsenv::build_env_vars;
use rsenv::envrc::{
    delete_section, update_dot_envrc, END_SECTION_DELIMITER, START_SECTION_DELIMITER,
};
use rsenv::errors::TreeResult;
use rstest::{fixture, rstest};
use tempfile::tempdir;

#[fixture]
fn temp_dir() -> PathBuf {
    let tempdir = tempdir().unwrap();
    let options = dir::CopyOptions::new();
    copy_items(
        &["tests/resources/environments/complex/dot.envrc"],
        tempdir.path(),
        &options,
    )
    .expect("Failed to copy test project directory");

    tempdir.into_path()
}

fn get_file_contents(path: &Path) -> TreeResult<String> {
    let mut file = fs::File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

#[rstest]
fn given_envrc_file_when_updating_then_adds_correct_section(temp_dir: PathBuf) -> TreeResult<()> {
    let path = temp_dir.join("dot.envrc");
    let data = build_env_vars(Path::new(
        "./tests/resources/environments/complex/level4.env",
    ))?;

    update_dot_envrc(&path, &data)?;

    let file_contents = get_file_contents(&path)?;
    let conf_guard_start = file_contents.find(START_SECTION_DELIMITER).unwrap();
    let conf_guard_end = file_contents.find(END_SECTION_DELIMITER).unwrap();
    let conf_guard_section = &file_contents[conf_guard_start..conf_guard_end];

    assert!(conf_guard_section.contains(START_SECTION_DELIMITER));
    assert!(conf_guard_section.contains(&data));
    assert!(path.exists());
    println!("file_contents: {}", file_contents);
    Ok(())
}

#[rstest]
fn given_envrc_with_section_when_deleting_then_removes_section(
    temp_dir: PathBuf,
) -> TreeResult<()> {
    let path = temp_dir.join("dot.envrc");
    let data = build_env_vars(Path::new(
        "./tests/resources/environments/complex/level4.env",
    ))?;

    // Given: section has been added
    update_dot_envrc(&path, &data)?;

    // When: section is deleted
    delete_section(&path)?;

    let file_contents = get_file_contents(&path)?;
    assert!(!file_contents.contains(START_SECTION_DELIMITER));
    assert!(!file_contents.contains(&data));
    println!("file_contents: {}", file_contents);
    Ok(())
}

#[rstest]
fn given_multiple_updates_when_updating_envrc_then_maintains_single_section(
    temp_dir: PathBuf,
) -> TreeResult<()> {
    let path = temp_dir.join("dot.envrc");
    let data1 = build_env_vars(Path::new(
        "./tests/resources/environments/complex/level4.env",
    ))?;
    let data2 = build_env_vars(Path::new(
        "./tests/resources/environments/complex/a/level3.env",
    ))?;

    // First update
    update_dot_envrc(&path, &data1)?;
    // Second update should replace the first section
    update_dot_envrc(&path, &data2)?;

    let file_contents = get_file_contents(&path)?;
    assert!(file_contents.contains(&data2));
    assert!(!file_contents.contains(&data1));

    // Should only have one set of delimiters
    assert_eq!(file_contents.matches(START_SECTION_DELIMITER).count(), 1);
    assert_eq!(file_contents.matches(END_SECTION_DELIMITER).count(), 1);

    Ok(())
}
