#![allow(unused_imports)]

use std::collections::{BTreeMap};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use anyhow::{Context, Result};
use log::{debug, info};
use std::env;
use camino::{Utf8Path, Utf8PathBuf};
use regex::Regex;
use stdext::function_name;

pub const START_SECTION_DELIMITER: &str = "#------------------------------- rsenv start --------------------------------";
pub const END_SECTION_DELIMITER: &str = "#-------------------------------- rsenv end ---------------------------------";

pub fn update_dot_envrc(target_file_path: &Utf8Path, data: &str) -> Result<()> {

    let section = format!(
        "{start_section_delimiter}\n\
         {data}\
         {end_section_delimiter}\n",
        start_section_delimiter = START_SECTION_DELIMITER,
        data = data,
        end_section_delimiter = END_SECTION_DELIMITER,
    );

    let file = File::open(target_file_path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;

    let start_index = lines.iter().position(|l| {
        l.starts_with(START_SECTION_DELIMITER)
    });
    let end_index = lines.iter().position(|l| {
        l.starts_with(END_SECTION_DELIMITER)
    });

    let mut new_file_content = String::new();

    match (start_index, end_index) {
        (Some(start), Some(end)) if start < end => {
            new_file_content.push_str(&lines[..start].join("\n"));
            new_file_content.push_str(&section);
            new_file_content.push_str(&lines[end + 1..].join("\n"));
        }
        _ => {
            new_file_content.push_str(&lines.join("\n"));
            new_file_content.push_str(&section);
        }
    }

    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(target_file_path)?;

    file.write_all(new_file_content.as_bytes())?;
    Ok(())
}
pub fn delete_section(file_path: &Utf8Path) -> Result<()> {
    // Read the file to a String
    let mut file = File::open(file_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Define the regex
    // (?s) enables "single-line mode" where . matches any character including newline (\n), allows to span lines It's often also called "dotall mode".
    // In this case, we want to match across multiple lines, hence the s modifier is used.
    let re = Regex::new(r"(?s)#------------------------------- confguard start --------------------------------.*#-------------------------------- confguard end ---------------------------------\n").unwrap();

    // Replace the matched section with an empty string
    let result = re.replace(&contents, "");

    // Write the result back to the file
    let mut file = File::create(file_path)?;
    file.write_all(result.as_bytes())?;

    Ok(())
}
