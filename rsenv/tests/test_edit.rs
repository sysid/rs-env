use std::io::Write;
use std::path::Path;
use std::process::Command;

use rstest::rstest;

use rsenv::builder::TreeBuilder;
use rsenv::edit::{
    create_branches, create_vimscript, open_files_in_editor, select_file_with_suffix,
};
use rsenv::errors::TreeResult;
use rsenv::get_files;

#[rstest]
#[ignore = "Interactive via Makefile"]
fn given_directory_when_selecting_file_with_suffix_then_returns_valid_file() -> TreeResult<()> {
    let dir = Path::new("./tests/resources/data");
    let suffix = ".env";
    let result = select_file_with_suffix(dir, suffix)?;
    println!("Selected: {}", result.display());
    assert!(result.to_string_lossy().ends_with(suffix));
    Ok(())
}

#[rstest]
#[ignore = "Interactive via Makefile"]
fn given_valid_files_when_opening_in_editor_then_opens_successfully() -> TreeResult<()> {
    let files = get_files(Path::new(
        "./tests/resources/environments/complex/level4.env",
    ))?;
    open_files_in_editor(files)?;
    Ok(())
}

#[rstest]
#[ignore = "Interactive via Makefile"]
fn given_file_list_when_creating_vimscript_then_generates_valid_interactive_script(
) -> TreeResult<()> {
    let files = [
        vec!["a_test.env", "b_test.env", "test.env"],
        vec!["a_int.env", "b_int.env", "int.env"],
        vec!["a_prod.env"],
    ];

    let script = create_vimscript(
        files
            .iter()
            .map(|v| v.iter().map(Path::new).collect())
            .collect(),
    );
    println!("{}", script);

    // Save script to file
    let vimscript_filename = "tests/resources/environments/generated.vim";
    let mut file = std::fs::File::create(vimscript_filename)?;
    file.write_all(script.as_bytes())?;

    // Run vim with the generated script
    let status = Command::new("vim")
        .arg("-S")
        .arg(vimscript_filename)
        .status()?;

    println!("Vim exited with status: {:?}", status);
    Ok(())
}

#[rstest]
fn given_file_list_when_creating_vimscript_then_generates_expected_script() {
    let files = [
        vec!["a_test.env", "b_test.env", "test.env"],
        vec!["a_int.env", "b_int.env", "int.env"],
        vec!["a_prod.env"],
    ];

    let script = create_vimscript(
        files
            .iter()
            .map(|v| v.iter().map(Path::new).collect())
            .collect(),
    );

    let expected = "\
\" Open the first set of files ('a_test.env') in the first column
edit a_test.env
split b_test.env
split test.env
split a_int.env
\" move to right column
wincmd L
split b_int.env
split int.env
split a_prod.env
\" move to right column
wincmd L

\" make distribution equal
wincmd =

\" jump to left top corner
1wincmd w
";

    assert_eq!(script, expected);
}

#[rstest]
fn given_tree_structure_when_creating_branches_then_returns_correct_branch_paths() -> TreeResult<()>
{
    let mut builder = TreeBuilder::new();
    let trees = builder.build_from_directory(Path::new("./tests/resources/environments/tree"))?;
    let mut result: Vec<Vec<String>> = create_branches(&trees)
        .into_iter()
        .map(|branch| {
            branch
                .into_iter()
                .map(|path| {
                    path.file_name()
                        .expect("Invalid path")
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        })
        .collect();

    // Sort both result and expected for stable comparison
    result.sort();

    let mut expected = vec![
        vec!["level11.env", "root.env"],
        vec!["level13.env", "root.env"],
        vec!["level32.env", "level22.env", "level12.env", "root.env"],
        vec!["level21.env", "level12.env", "root.env"],
    ];
    expected.sort();
    assert_eq!(result, expected);
    Ok(())
}

#[rstest]
fn given_parallel_structure_when_creating_branches_then_returns_correct_paths() -> TreeResult<()> {
    let mut builder = TreeBuilder::new();
    let trees =
        builder.build_from_directory(Path::new("./tests/resources/environments/parallel"))?;
    let mut result: Vec<Vec<String>> = create_branches(&trees)
        .into_iter()
        .map(|branch| {
            branch
                .into_iter()
                .map(|path| {
                    path.file_name()
                        .expect("Invalid path")
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        })
        .collect();
    result.sort();

    let mut expected = vec![
        vec!["int.env", "b_int.env", "a_int.env"],
        vec!["prod.env", "b_prod.env", "a_prod.env"],
        vec!["test.env", "b_test.env", "a_test.env"],
    ];
    expected.sort();
    assert_eq!(result, expected);
    Ok(())
}

#[rstest]
fn given_complex_structure_when_creating_branches_then_returns_correct_hierarchy() -> TreeResult<()>
{
    let mut builder = TreeBuilder::new();
    let trees =
        builder.build_from_directory(Path::new("./tests/resources/environments/complex"))?;
    let mut result: Vec<Vec<String>> = create_branches(&trees)
        .into_iter()
        .map(|branch| {
            branch
                .into_iter()
                .map(|path| {
                    path.file_name()
                        .expect("Invalid path")
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        })
        .collect();
    result.sort();

    let mut expected = vec![vec![
        "level4.env",
        "level3.env",
        "level2.env",
        "level1.env",
        "dot.envrc",
    ]];
    expected.sort();
    assert_eq!(result, expected);
    Ok(())
}
