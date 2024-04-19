use std::{
    iter,
    path::{Path, PathBuf},
};

mod parsing;

struct FormatPart<'a> {
    expression: &'a str,
    formatting: Option<&'a str>,
}

impl<'a> From<&'a str> for FormatPart<'a> {
    fn from(value: &'a str) -> Self {
        if let Some(position) = value.find(':') {
            let (expression, formatting) = value.split_at(position);
            FormatPart {
                expression,
                formatting: Some(formatting),
            }
        } else {
            FormatPart {
                expression: value,
                formatting: None,
            }
        }
    }
}

impl<'a> FormatPart<'a> {
    fn to_code(&self) -> String {
        match self {
            FormatPart {
                expression,
                formatting: Some(format_part),
            } => format!(
                r#"result.push_str(&format!("{{{}}}", {}));"#,
                format_part, expression
            ),
            FormatPart {
                expression,
                formatting: None,
            } => format!(r#"result.push_str(&format!("{{{}}}"));"#, expression),
        }
    }
}

fn obtain_format_part(code_block: &str) -> (Option<&str>, Option<FormatPart>) {
    match code_block.rfind(';') {
        Some(position) => {
            let format_part = match code_block[(position + 1)..].trim() {
                "" => None,
                other => Some(other),
            };

            (
                Some(&code_block[..position + 1]),
                format_part.map(FormatPart::from),
            )
        }
        None => {
            let format_part = match code_block.trim() {
                "" => None,
                other => Some(other),
            };

            (None, format_part.map(FormatPart::from))
        }
    }
}

fn handle_input(input: &str) -> Result<String, parsing::MatchError> {
    let parsing::ParseResult {
        mut code_block_fragments,
        mut template_fragments,
    } = parsing::parse_template(input)?;

    let mut code = format!(
        r#"let mut result = String::from("{}");"#,
        &template_fragments.pop_front().unwrap()
    );
    let end = "result";

    if let Some(code_block) = &code_block_fragments.pop_front() {
        match obtain_format_part(code_block) {
            (None, None) => unreachable!(),
            (None, Some(format_part)) => {
                code.push_str(&format_part.to_code());
            }
            (Some(code_block), None) => {
                code.push_str(code_block);
            }
            (Some(code_block), Some(format_part)) => {
                code.push_str(code_block);
                code.push_str(&format_part.to_code());
            }
        }
    }

    for (template, code_block) in iter::zip(&template_fragments, &code_block_fragments) {
        code.push_str(&format!(r#"result.push_str("{}");"#, template));

        match obtain_format_part(code_block) {
            (None, None) => unreachable!(),
            (None, Some(format_part)) => {
                code.push_str(&format_part.to_code());
            }
            (Some(code_block), None) => {
                code.push_str(code_block);
            }
            (Some(code_block), Some(format_part)) => {
                code.push_str(code_block);
                code.push_str(&format_part.to_code());
            }
        }
    }

    if let Some(template_part) = template_fragments.pop_back() {
        code.push_str(&format!(r#"result.push_str("{}");"#, template_part));
    }
    //

    code.push_str(end);

    Ok(code)
}

fn create_include_bytes(file_path: &PathBuf) -> String {
    format!(r#"include_bytes!({:?});"#, file_path)
}

#[derive(Debug)]
struct PathResolutionError {
    path: PathBuf,
    source: std::io::Error,
}

impl std::fmt::Display for PathResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{} - {:?}", self.source, self.path))
    }
}

impl From<(PathBuf, std::io::Error)> for PathResolutionError {
    fn from(value: (PathBuf, std::io::Error)) -> Self {
        Self {
            path: value.0,
            source: value.1,
        }
    }
}

fn canonicalize_path<P>(path: P) -> Result<PathBuf, PathResolutionError>
where
    P: AsRef<Path>,
{
    let mut canonicalized_path = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR environment variable"),
    );
    canonicalized_path.push(path.as_ref());

    std::fs::canonicalize(canonicalized_path.clone()).map_err(|e| (canonicalized_path, e).into())
}

#[proc_macro]
pub fn remplate(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input_str = input.to_string();
    let template_path = input_str.trim_matches('"');

    let canonicalized_path = match canonicalize_path(template_path) {
        Ok(path) => path,
        Err(error) => panic!("{}", error),
    };

    let file_content = match std::fs::read_to_string(&canonicalized_path) {
        Ok(content) => content,
        Err(error) => panic!("{:?}", error),
    };

    format!(
        r"{{
            {}
            {}
        }}",
        create_include_bytes(&canonicalized_path),
        match handle_input(&file_content) {
            Ok(definition) => definition,
            Err(error) => error.abort_with_error(),
        },
    )
    .parse()
    .unwrap()
}
