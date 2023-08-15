#![allow(unused_imports)]

use std::collections::BTreeMap;
use anyhow::Result;
use rstest::rstest;
use rsenv::{build_env, dlog, extract_env, build_env_vars, print_files};
use log::{debug, info};
use stdext::function_name;

#[ctor::ctor]
fn init() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::max())
        .is_test(true)
        .try_init();
}

#[rstest]
fn test_extract_env() -> Result<()> {
    let (variables, parent) = extract_env("./tests/resources/data/level4.env")?;
    dlog!("variables: {:?}", variables);
    dlog!("parent: {:?}", parent);
    assert_eq!(variables.get("VAR_6"), Some(&"var_64".to_string()));
    // assert_eq!(parent, Some("a/level3.env".to_string()));
    Ok(())
}

#[rstest]
fn test_build_env() -> Result<()> {
    let (variables, files) = build_env("./tests/resources/data/level4.env")?;
    let reference = extract_env("./tests/resources/data/result.env")?.0;
    // println!("reference: {:#?}", reference);
    // println!("variables: {:#?}", variables);
    let filtered_map: BTreeMap<_, _> = variables.iter()
        .filter(|(k, _)| k.starts_with("VAR_"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    println!("variables: {:#?}", filtered_map);
    println!("files: {:#?}", files);

    assert_eq!(filtered_map, reference, "The two BTreeMaps are not equal!");
    Ok(())
}

#[rstest]
fn test_build_env_vars() -> Result<()> {
    let env_vars = build_env_vars("./tests/resources/data/level4.env")?;
    println!("{}", env_vars);
    Ok(())
}

#[rstest]
fn test_print_files() -> Result<()> {
    print_files("./tests/resources/data/level4.env")?;
    Ok(())
}
