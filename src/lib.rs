use std::{
    iter,
    path::{Path, PathBuf},
};

mod parsing;

enum TemplateExpression<'a> {
    CodeBlock(&'a str),
    CodeBlockWithFormattable(&'a str, Formattable<'a>),
    Formattable(Formattable<'a>),
}

impl<'a> TemplateExpression<'a> {
    fn to_code(&self) -> String {
        match self {
            TemplateExpression::CodeBlock(code) => code.to_string(),
            TemplateExpression::CodeBlockWithFormattable(code, formattable) => {
                format!("{}\n{}", code, formattable.to_code())
            }
            TemplateExpression::Formattable(formattable) => formattable.to_code(),
        }
    }
}

impl<'a> TryFrom<&'a str> for TemplateExpression<'a> {
    type Error = ();

    fn try_from(code_block: &'a str) -> Result<Self, Self::Error> {
        match code_block.rfind(';') {
            Some(position) => match code_block[(position + 1)..].trim() {
                "" => Ok(TemplateExpression::CodeBlock(&code_block[..position + 1])),
                format_part => Ok(TemplateExpression::CodeBlockWithFormattable(
                    &code_block[..position + 1],
                    Formattable::from(format_part),
                )),
            },
            None => match code_block.trim() {
                "" => Err(()),
                format_part => Ok(TemplateExpression::Formattable(Formattable::from(
                    format_part,
                ))),
            },
        }
    }
}

struct Formattable<'a> {
    expression: &'a str,
    formatting: Option<&'a str>,
}

impl<'a> From<&'a str> for Formattable<'a> {
    fn from(value: &'a str) -> Self {
        if let Some(position) = value.find(':') {
            let (expression, formatting) = value.split_at(position);
            Formattable {
                expression,
                formatting: Some(formatting),
            }
        } else {
            Formattable {
                expression: value,
                formatting: None,
            }
        }
    }
}

impl<'a> Formattable<'a> {
    fn to_code(&self) -> String {
        match self {
            Formattable {
                expression,
                formatting: Some(format_part),
            } => format!(
                r#"result.write_fmt(format_args!("{{{}}}", {}))?;"#,
                format_part, expression
            ),
            Formattable {
                expression,
                formatting: None,
            } => format!(r#"result.write_fmt(format_args!("{{{}}}"))?;"#, expression),
        }
    }
}

fn handle_input(input: &str) -> Result<String, parsing::MatchError> {
    let parsing::ParseResult {
        code_block_fragments,
        template_fragments,
    } = parsing::parse_template(input)?;

    let template_size_estimation = (template_fragments
        .iter()
        .fold(0, |acc, fragment| acc + fragment.len()))
        + (code_block_fragments.len() * core::mem::size_of::<i64>() * 2);

    let mut code = format!(
        r#"use ::core::fmt::Write;
        let mut result = String::with_capacity({});"#,
        template_size_estimation
    );

    code.push_str(&format!(
        r#"result.write_str("{}")?;"#,
        &template_fragments.first().unwrap()
    ));

    let end = "::core::result::Result::Ok::<::std::string::String, ::core::fmt::Error>(result)";

    if let Some(code_block) = code_block_fragments.first() {
        if let Ok(expression) = TemplateExpression::try_from(*code_block) {
            code.push_str(&expression.to_code());
        }
    }

    for (template, code_block) in iter::zip(&template_fragments, &code_block_fragments).skip(1) {
        code.push_str(&format!(r#"result.write_str("{}")?;"#, template));

        if let Ok(expression) = TemplateExpression::try_from(*code_block) {
            code.push_str(&expression.to_code());
        }
    }

    if let Some(template_part) = template_fragments.last() {
        code.push_str(&format!(r#"result.write_str("{}")?;"#, template_part));
    }

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
        r"(||{{
            {}
            {}
        }})()",
        create_include_bytes(&canonicalized_path),
        match handle_input(&file_content) {
            Ok(definition) => definition,
            Err(error) => error.abort_with_error(),
        },
    )
    .parse()
    .unwrap()
}
