#![allow(unused_imports)]

use std::fs;
use std::io::Read;
use camino::{Utf8Path, Utf8PathBuf};
use anyhow::{Context, Result};

use camino_tempfile::{NamedUtf8TempFile, tempdir};
use fs_extra::{copy_items, dir, remove_items};
use itertools::Itertools;
use log::{debug, warn};
use stdext::function_name;
use rstest::{fixture, rstest};
use rsenv::build_env_vars;
use rsenv::envrc::{END_SECTION_DELIMITER, START_SECTION_DELIMITER, update_dot_envrc};

#[fixture]
fn temp_dir() -> Utf8PathBuf {
    let tempdir = tempdir().unwrap();
    let options = dir::CopyOptions::new(); //Initialize default values for CopyOptions
    copy_items(
        &[
            "tests/resources/data/dot.envrc",
        ],
        &tempdir,
        &options,
    )
        .expect("Failed to copy test project directory");

    tempdir.into_path()
}

pub fn get_file_contents(path: &Utf8Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}
#[rstest]
fn test_update_dot_envrc(temp_dir: Utf8PathBuf) -> Result<()> {
    let path = temp_dir.join("./dot.envrc");
    let data = build_env_vars("./tests/resources/data/level4.env")?;

    update_dot_envrc(&path, data.as_str()).unwrap();

    let file_contents = get_file_contents(&path).unwrap();
    let conf_guard_start = file_contents
        .find(START_SECTION_DELIMITER)
        .unwrap();
    let conf_guard_end = file_contents
        .find(END_SECTION_DELIMITER)
        .unwrap();
    let conf_guard_section = &file_contents[conf_guard_start..conf_guard_end];
    assert!(conf_guard_section.contains(START_SECTION_DELIMITER));
    assert!(conf_guard_section.contains(data.as_str()));
    assert!(path.exists());
    println!("file_contents: {}", file_contents);
    Ok(())
}

