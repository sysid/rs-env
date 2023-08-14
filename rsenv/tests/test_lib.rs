#![allow(unused_imports)]

use std::collections::BTreeMap;
use anyhow::Result;
use rstest::rstest;
use rsenv::{build_env, dlog, extract_env, print_env};
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
    let variables = build_env("./tests/resources/data/level4.env")?;
    let reference = extract_env("./tests/resources/data/result.env")?.0;
    // println!("reference: {:#?}", reference);
    // println!("variables: {:#?}", variables);
    let filtered_map: BTreeMap<_, _> = variables.iter()
        .filter(|(k, _)| k.starts_with("VAR_"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    println!("variables: {:#?}", filtered_map);

    assert_eq!(filtered_map, reference, "The two BTreeMaps are not equal!");
    Ok(())
}

#[rstest]
fn test_print_env() -> Result<()> {
    print_env("./tests/resources/data/level4.env")?;
    Ok(())
}
