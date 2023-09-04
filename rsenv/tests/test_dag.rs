#![allow(unused_imports)]

use std::collections::BTreeMap;
use std::fs;
use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use camino_tempfile::tempdir;
use fs_extra::{copy_items, dir};
use rstest::{fixture, rstest};
use rsenv::{build_env, dlog, extract_env, build_env_vars, print_files, link, link_all, unlink};
use log::{debug, info};
use stdext::function_name;
use rsenv::dag::build_dag;

#[ctor::ctor]
fn init() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::max())
        .is_test(true)
        .try_init();
}

#[rstest]
#[ignore = "not implemented yet"]
fn test_build_dag() -> Result<()> {
    let dag = build_dag(Utf8Path::new("./tests/resources/environments/graph/level31.env"))?;
    println!("{:#?}", dag);
    Ok(())
}
