#![allow(unused_imports)]
use nom::bytes::complete::{tag, take_while};
use nom::error::{dbg_dmp, Error, ParseError};
use nom::{AsBytes, IResult, Parser};
use nom::character::complete::multispace0;
use nom::sequence::delimited;
use tracing::{debug, instrument};

// Parser to skip whitespace
#[allow(dead_code)]
#[instrument(level = "trace")]
fn space(input: &str) -> IResult<&str, &str> {
    take_while(|c: char| c.is_whitespace())(input)
}

/// A combinator that takes a parser `inner` and produces a parser that also consumes both leading and
/// trailing whitespace, returning the output of `inner`.
#[allow(dead_code)]
fn ws<'a, F, O, E: ParseError<&'a str>>(inner: F) -> impl FnMut(&'a str) -> IResult<&'a str, O, E>
    where
        F: Parser<&'a str, O, E>,
{
    delimited(
        multispace0,
        inner,
        multispace0,
    )
}

// Parser to extract the path after `# rsenv:`
#[allow(dead_code)]
#[instrument(level = "trace")]
pub fn extract_path(input: &str) -> IResult<&str, &str> {
    debug!("input: {:?}", input);
    // dbg_dmp(tag::<&str, &[u8], Error<_>>("# rsenv:"),"xxx")(input.as_bytes());

    let (input, _) = multispace0(input)?; // Match optional whitespace or newlines
    let (input, _) = tag("# rsenv:")(input)?;
    debug!("input: {:?}", input);
    // let (input, _) = space(input)?;
    // debug!("input: {:?}", input);
    ws(take_while(|c: char| !c.is_whitespace()))(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::{fixture, rstest};
    use crate::parser::extract_path;

    #[test]
    fn test_extract_path() {
        let content = r#"
# rsenv: level1.env

# Level2 overwrite
export VAR_4=var_42
export VAR_5=var_52
"#;

        match extract_path(content) {
            Ok((_, path)) => println!("Extracted path: {}", path),
            Err(e) => println!("Error: {:?}", e),
        }
    }
}
