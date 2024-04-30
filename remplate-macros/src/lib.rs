use std::{
    iter,
    path::{Path, PathBuf},
    str::FromStr,
};

use syn::{parse::Parse, spanned::Spanned, LitStr, Token};

mod parsing;

enum TemplateExpression<'a> {
    CodeBlock(&'a str),
    CodeBlockWithFormattable(&'a str, Formattable<'a>),
    Formattable(Formattable<'a>),
}

impl<'a> TemplateExpression<'a> {
    fn to_code(&self) -> proc_macro2::TokenStream {
        match self {
            TemplateExpression::CodeBlock(code) => {
                proc_macro2::TokenStream::from_str(code).unwrap()
            }
            TemplateExpression::CodeBlockWithFormattable(code, formattable) => {
                let mut code = proc_macro2::TokenStream::from_str(code).unwrap();
                code.extend(formattable.to_code());
                code
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
    fn to_code(&self) -> proc_macro2::TokenStream {
        match self {
            Formattable {
                expression,
                formatting: Some(format_part),
            } => {
                let format_part = format!("{{{}}}", format_part);
                let expression = proc_macro2::TokenStream::from_str(expression).unwrap();

                quote::quote! {
                    f.write_fmt(format_args!(#format_part, #expression))?;
                }
            }
            Formattable {
                expression,
                formatting: None,
            } => {
                let expression = proc_macro2::TokenStream::from_str(expression).unwrap();

                quote::quote! {
                    f.write_fmt(format_args!("{}", #expression))?;
                }
            }
        }
    }
}

fn handle_input(
    input: &str,
) -> Result<(proc_macro2::TokenStream, proc_macro2::TokenStream), parsing::MatchError> {
    let parsing::ParseResult {
        code_block_fragments,
        template_fragments,
    } = parsing::parse_template(input)?;

    let template_size_estimation = (template_fragments
        .iter()
        .fold(0, |acc, fragment| acc + fragment.len()))
        + (code_block_fragments.len() * core::mem::size_of::<i64>() * 2);

    let string_allocation_part = quote::quote! {
        let mut f = String::with_capacity(#template_size_estimation);
    };

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
            code.extend(expression.to_code());
        }
    }

    for (template, code_block) in iter::zip(&template_fragments, &code_block_fragments).skip(1) {
        code.extend(quote::quote! {
            f.write_str(#template)?;
        });

        if let Ok(expression) = TemplateExpression::try_from(*code_block) {
            code.extend(expression.to_code());
        }
    }

    if let Some(template_part) = template_fragments.last() {
        code.extend(quote::quote! {
            f.write_str(#template_part)?;
        });
    }

    code.extend(end);

    Ok((string_allocation_part, code))
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
    string_allocation_part: proc_macro2::TokenStream,
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

    let (string_allocation_part, code) = match handle_input(&file_content) {
        Ok(definition) => definition,
        Err(error) => error.abort_with_error(),
    };

    quote::quote! {};

    RemplateResult {
        string_allocation_part,
        remplate_code: code,
        include_bytes_part: create_include_bytes(&canonicalized_path),
    }
}

#[proc_macro]
pub fn remplate(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input_str = input.to_string();
    let template_path = input_str.trim_matches('"');

    let RemplateResult {
        string_allocation_part,
        remplate_code,
        include_bytes_part,
    } = handle_remplate_path(template_path);

    quote::quote! {
        (||{
            #include_bytes_part
            #string_allocation_part
            #remplate_code
            ::core::result::Result::Ok::<::std::string::String, ::core::fmt::Error>(f)
        })()
    }
    .into()
}

mod kw {
    syn::custom_keyword!(path);
}

struct RemplatePath(String);

impl Parse for RemplatePath {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        input.parse::<kw::path>()?;
        input.parse::<Token![=]>()?;
        let path_parameter: LitStr = input.parse()?;

        Ok(Self(path_parameter.value()))
    }
}

#[proc_macro_derive(Remplate, attributes(remplate))]
pub fn derive_remplate(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(item as syn::DeriveInput);
    let input_span = input.span();
    let impl_type = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let template_path = match input.attrs.into_iter().find(|attr| {
        let attr_path = attr.meta.path();
        attr_path.is_ident("remplate")
    }) {
        Some(attr) => match attr
            .meta
            .require_list()
            .map(|meta_list| meta_list.tokens.clone())
            .and_then(|tokens| syn::parse2::<RemplatePath>(tokens))
        {
            Ok(path) => path.0,
            Err(error) => return error.to_compile_error().into(),
        },
        None => {
            return syn::parse::Error::new(input_span, "Missing template path")
                .to_compile_error()
                .into()
        }
    };

    let RemplateResult {
        string_allocation_part: _,
        remplate_code,
        include_bytes_part,
    } = handle_remplate_path(&template_path);

    quote::quote! {
        impl #impl_generics ::core::fmt::Display for #impl_type #ty_generics #where_clause {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                #include_bytes_part
                #remplate_code
                Ok(())
            }
        }
        impl #impl_generics ::remplate::Remplate for #impl_type #ty_generics #where_clause {};
    }
    .into()
}
