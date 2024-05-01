use std::{
    iter,
    ops::Range,
    path::{Path, PathBuf},
    str::FromStr,
};

use error::TemplateError;
use macro_parsing::MacroParseResult;
use quote::ToTokens;

mod error;
mod macro_parsing;
mod template_parsing;

enum TemplateExpression<'a> {
    CodeBlock(&'a str, Range<usize>),
    CodeBlockWithFormattable((&'a str, Range<usize>), Formattable<'a>),
    Formattable(Formattable<'a>),
}

impl<'a> ToTokens for TemplateExpression<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        match self {
            TemplateExpression::CodeBlock(template, code_block_range) => {
                match proc_macro2::TokenStream::from_str(&template[code_block_range.clone()]) {
                    Ok(code) => tokens.extend(code),
                    Err(error) => tokens.extend(
                        syn::Error::new(error.span(), error.to_string()).to_compile_error(),
                    ),
                }
            }
            TemplateExpression::CodeBlockWithFormattable(
                (template, code_block_range),
                formattable,
            ) => {
                match proc_macro2::TokenStream::from_str(&template[code_block_range.clone()]) {
                    Ok(code) => tokens.extend(code),
                    Err(error) => tokens.extend(
                        syn::Error::new(error.span(), error.to_string()).to_compile_error(),
                    ),
                }
                formattable.to_tokens(tokens);
            }
            TemplateExpression::Formattable(formattable) => formattable.to_tokens(tokens),
        }
    }
}

impl<'a> TryFrom<(&'a str, Range<usize>)> for TemplateExpression<'a> {
    type Error = ();

    fn try_from(
        (template, code_block_range): (&'a str, Range<usize>),
    ) -> Result<Self, Self::Error> {
        let code_block = &template[code_block_range.clone()];

        match code_block.rfind(';') {
            Some(position) => match code_block[(position + 1)..].trim() {
                "" => Ok(TemplateExpression::CodeBlock(
                    template,
                    code_block_range.start..(code_block_range.start + position + 1),
                )),
                _ => Ok(TemplateExpression::CodeBlockWithFormattable(
                    (
                        template,
                        code_block_range.start..(code_block_range.start + position + 1),
                    ),
                    Formattable::from((
                        template,
                        (code_block_range.start + position + 1)..code_block_range.end,
                    )),
                )),
            },
            None => match code_block.trim() {
                "" => Err(()),
                _ => Ok(TemplateExpression::Formattable(Formattable::from((
                    template,
                    code_block_range,
                )))),
            },
        }
    }
}

struct Formattable<'a> {
    expression: &'a str,
    formatting: Option<&'a str>,
}

impl<'a> From<(&'a str, Range<usize>)> for Formattable<'a> {
    fn from((template, expression_range): (&'a str, Range<usize>)) -> Self {
        let format_expression = &template[expression_range.clone()];

        if let Some(position) = format_expression.find(':') {
            let (expression, formatting) = format_expression.split_at(position);
            Formattable {
                expression,
                formatting: Some(formatting),
            }
        } else {
            Formattable {
                expression: format_expression,
                formatting: None,
            }
        }
    }
}

impl<'a> ToTokens for Formattable<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        tokens.extend(match self {
            Formattable {
                expression,
                formatting: Some(format_part),
            } => {
                let format_part = format!("{{{}}}", format_part);

                let expression = if expression.trim().is_empty() {
                    quote::quote! {
                        ::core::compile_error!("A format expression misses a value")
                    }
                } else {
                    match proc_macro2::TokenStream::from_str(expression) {
                        Ok(code) => code,
                        Err(error) => {
                            syn::Error::new(error.span(), error.to_string()).to_compile_error()
                        }
                    }
                };

                quote::quote! {
                    f.write_fmt(format_args!(#format_part, #expression))?;
                }
            }
            Formattable {
                expression,
                formatting: None,
            } => {
                let expression = match proc_macro2::TokenStream::from_str(expression) {
                    Ok(code) => code,
                    Err(error) => syn::Error::new(
                        error.span(),
                        format!("Invalid expression - '{}'", expression),
                    )
                    .to_compile_error(),
                };

                quote::quote! {
                    f.write_fmt(format_args!("{}", #expression))?;
                }
            }
        })
    }
}

fn create_code<'a>(
    template: &'a str,
    template_path: &'a PathBuf,
) -> Result<(usize, proc_macro2::TokenStream), TemplateError<'a>> {
    let template_parsing::ParseResult {
        code_block_fragment_ranges,
        template_fragment_ranges,
    } = template_parsing::parse_template(template, template_path)?;

    let estimated_template_size = (template_fragment_ranges
        .iter()
        .fold(0, |acc, fragment| acc + fragment.len()))
        + (code_block_fragment_ranges.len() * core::mem::size_of::<i64>() * 2);

    let mut code = quote::quote! {
        use ::core::fmt::Write;
    };

    {
        let first_template_fragment = &template[template_fragment_ranges.first().unwrap().clone()];
        code.extend(quote::quote! {
            f.write_str(#first_template_fragment)?;
        });
    }

    let end = quote::quote! {};

    if let Some(block_range) = code_block_fragment_ranges.first() {
        if let Ok(expression) = TemplateExpression::try_from((template, block_range.clone())) {
            expression.to_tokens(&mut code);
        }
    }

    for (template_fragment_range, block_range) in
        iter::zip(&template_fragment_ranges, &code_block_fragment_ranges).skip(1)
    {
        let template_fragment = &template[template_fragment_range.clone()];
        code.extend(quote::quote! {
            f.write_str(#template_fragment)?;
        });

        if let Ok(expression) = TemplateExpression::try_from((template, block_range.clone())) {
            expression.to_tokens(&mut code);
        }
    }

    if let Some(template_fragment_range) = template_fragment_ranges.last() {
        let template_fragment = &template[template_fragment_range.clone()];
        code.extend(quote::quote! {
            f.write_str(#template_fragment)?;
        });
    }

    code.extend(end);

    Ok((estimated_template_size, code))
}

fn create_include_bytes(file_path: &PathBuf) -> proc_macro2::TokenStream {
    let file_path = file_path.to_string_lossy();

    quote::quote! {
        ::core::include_bytes!(#file_path);
    }
}

#[derive(Debug)]
enum PathCanonicalizationError {
    CargoManifestDirVariable(std::env::VarError),
    IoError {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for PathCanonicalizationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathCanonicalizationError::CargoManifestDirVariable(error) => {
                f.write_fmt(format_args!("CARGO_MANIFEST_DIR - {:?}", error))
            }
            PathCanonicalizationError::IoError { path, source } => {
                f.write_fmt(format_args!("{} - {:?}", source, path))
            }
        }
    }
}

impl From<(PathBuf, std::io::Error)> for PathCanonicalizationError {
    fn from(value: (PathBuf, std::io::Error)) -> Self {
        Self::IoError {
            path: value.0,
            source: value.1,
        }
    }
}

fn canonicalize_path<P>(path: P) -> Result<PathBuf, PathCanonicalizationError>
where
    P: AsRef<Path>,
{
    let mut canonicalized_path = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR")
            .map_err(PathCanonicalizationError::CargoManifestDirVariable)?,
    );
    canonicalized_path.push(path.as_ref());

    std::fs::canonicalize(canonicalized_path.clone()).map_err(|e| (canonicalized_path, e).into())
}

struct RemplateData {
    estimated_template_size: usize,
    remplate_code: proc_macro2::TokenStream,
}

fn handle_template<'a>(
    template: &'a str,
    template_path: &'a PathBuf,
) -> Result<RemplateData, TemplateError<'a>> {
    let (estimated_template_size, code) = create_code(template, template_path)?;

    Ok(RemplateData {
        estimated_template_size,
        remplate_code: code,
    })
}

#[proc_macro_derive(Remplate, attributes(remplate))]
pub fn derive_remplate(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let MacroParseResult {
        impl_generics,
        type_generics,
        where_clause,
        type_ident,
        template_path,
    } = match macro_parsing::parse_derive_macro_input(item) {
        Ok(template_path) => template_path,
        Err(error) => return error.to_compile_error().into(),
    };

    let canonicalized_path = match canonicalize_path(template_path) {
        Ok(path) => path,
        Err(error) => {
            let message = format!("{}", error);
            return quote::quote! {
                ::core::compile_error!(#message);
            }
            .into();
        }
    };

    let template = match std::fs::read_to_string(&canonicalized_path) {
        Ok(content) => content,
        Err(error) => {
            let message = format!(
                "Unable to read template at {:?} - {}",
                canonicalized_path, error
            );
            return quote::quote! {
                ::core::compile_error!(#message);
            }
            .into();
        }
    };

    let RemplateData {
        estimated_template_size,
        remplate_code,
    } = match handle_template(&template, &canonicalized_path) {
        Ok(remplate_data) => remplate_data,
        Err(error) => return error.abortion_error().into(),
    };

    let include_bytes_part = create_include_bytes(&canonicalized_path);

    quote::quote! {
        impl #impl_generics ::core::fmt::Display for #type_ident #type_generics #where_clause {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                #include_bytes_part
                #remplate_code
                Ok(())
            }
        }
        impl #impl_generics ::remplate::Remplate for #type_ident #type_generics #where_clause {
            const ESTIMATED_SIZE: usize = #estimated_template_size;
        };
    }
    .into()
}
