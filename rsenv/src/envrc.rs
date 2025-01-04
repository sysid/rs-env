use std::path::Path;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use regex::Regex;
use tracing::{debug, instrument};
use crate::errors::{TreeError, TreeResult};
use crate::util::path::ensure_file_exists;

pub const START_SECTION_DELIMITER: &str = "#------------------------------- rsenv start --------------------------------";
pub const END_SECTION_DELIMITER: &str = "#-------------------------------- rsenv end ---------------------------------";

#[instrument(level = "debug")]
pub fn update_dot_envrc(target_file_path: &Path, data: &str) -> TreeResult<()> {
    ensure_file_exists(target_file_path)?;

    let section = format!(
        "\n{start_section_delimiter}\n\
         {data}\
         {end_section_delimiter}\n",
        start_section_delimiter = START_SECTION_DELIMITER,
        data = data,
        end_section_delimiter = END_SECTION_DELIMITER,
    );

    let file = File::open(target_file_path)
        .map_err(TreeError::FileReadError)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines()
        .collect::<Result<_, _>>()
        .map_err(TreeError::FileReadError)?;

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
        .open(target_file_path)
        .map_err(TreeError::FileReadError)?;

    file.write_all(new_file_content.as_bytes())
        .map_err(TreeError::FileReadError)
}

#[instrument(level = "debug")]
pub fn delete_section(file_path: &Path) -> TreeResult<()> {
    let mut file = File::open(file_path)
        .map_err(TreeError::FileReadError)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(TreeError::FileReadError)?;

    // Define the regex
    // (?s) enables "single-line mode" where . matches any character including newline (\n), allows to span lines It's often also called "dotall mode".
    // In this case, we want to match across multiple lines, hence the s modifier is used.
    // (?s)#--------------------- rsenv start ----------------------.*#---------------------- rsenv end -----------------------\n
    let pattern = format!(
        r"(?s){start_section_delimiter}.*{end_section_delimiter}\n",
        start_section_delimiter = START_SECTION_DELIMITER,
        end_section_delimiter = END_SECTION_DELIMITER,
    );
    debug!("pattern: {}", pattern);
    let re = Regex::new(pattern.as_str())
        .map_err(|e| TreeError::InternalError(e.to_string()))?;

    // Assert that only one section
    let result = re.find_iter(&contents).collect::<Vec<_>>();
    if result.len() > 1 {
        return Err(TreeError::MultipleParents(file_path.to_path_buf()));
    }

    // Replace the matched section with an empty string
    let result = re.replace(&contents, "");

    // Write the result back to the file
    let mut file = File::create(file_path)
        .map_err(TreeError::FileReadError)?;
    file.write_all(result.as_bytes())
        .map_err(TreeError::FileReadError)
}