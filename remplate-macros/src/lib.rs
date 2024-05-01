use std::{
    iter,
    path::{Path, PathBuf},
    str::FromStr,
};

use macro_parsing::MacroParseResult;
use quote::ToTokens;

mod macro_parsing;
mod template_parsing;

enum TemplateExpression<'a> {
    CodeBlock(&'a str),
    CodeBlockWithFormattable(&'a str, Formattable<'a>),
    Formattable(Formattable<'a>),
}

impl<'a> ToTokens for TemplateExpression<'a> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        match self {
            TemplateExpression::CodeBlock(code) => match proc_macro2::TokenStream::from_str(code) {
                Ok(code) => tokens.extend(code),
                Err(error) => tokens
                    .extend(syn::Error::new(error.span(), error.to_string()).to_compile_error()),
            },
            TemplateExpression::CodeBlockWithFormattable(code, formattable) => {
                match proc_macro2::TokenStream::from_str(code) {
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

fn handle_input(
    input: &str,
) -> Result<(usize, proc_macro2::TokenStream), template_parsing::MatchError> {
    let template_parsing::ParseResult {
        code_block_fragments,
        template_fragments,
    } = template_parsing::parse_template(input)?;

    let estimated_template_size = (template_fragments
        .iter()
        .fold(0, |acc, fragment| acc + fragment.len()))
        + (code_block_fragments.len() * core::mem::size_of::<i64>() * 2);

    let mut code = quote::quote! {
        use ::core::fmt::Write;
    };

    {
        let first_template_fragment = &template_fragments.first().unwrap();
        code.extend(quote::quote! {
            f.write_str(#first_template_fragment)?;
        });
    }

    let end = quote::quote! {};

    if let Some(code_block) = code_block_fragments.first() {
        if let Ok(expression) = TemplateExpression::try_from(*code_block) {
            expression.to_tokens(&mut code);
        }
    }

    for (template, code_block) in iter::zip(&template_fragments, &code_block_fragments).skip(1) {
        code.extend(quote::quote! {
            f.write_str(#template)?;
        });

        if let Ok(expression) = TemplateExpression::try_from(*code_block) {
            expression.to_tokens(&mut code);
        }
    }

    if let Some(template_part) = template_fragments.last() {
        code.extend(quote::quote! {
            f.write_str(#template_part)?;
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

struct RemplateResult {
    estimated_template_size: usize,
    remplate_code: proc_macro2::TokenStream,
    include_bytes_part: proc_macro2::TokenStream,
}

fn handle_remplate_path(path: &str) -> RemplateResult {
    let canonicalized_path = match canonicalize_path(path) {
        Ok(path) => path,
        Err(error) => panic!("{}", error),
    };

    let file_content = match std::fs::read_to_string(&canonicalized_path) {
        Ok(content) => content,
        Err(error) => panic!("{:?}", error),
    };

    let (estimated_template_size, code) = match handle_input(&file_content) {
        Ok(definition) => definition,
        Err(error) => error.abort_with_error(),
    };

    RemplateResult {
        estimated_template_size,
        remplate_code: code,
        include_bytes_part: create_include_bytes(&canonicalized_path),
    }
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

    let RemplateResult {
        estimated_template_size,
        remplate_code,
        include_bytes_part,
    } = handle_remplate_path(&template_path);

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
