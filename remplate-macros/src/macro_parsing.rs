use quote::ToTokens;
use syn::{parse::Parse, spanned::Spanned, DeriveInput, LitStr, Token};

mod kw {
    syn::custom_keyword!(path);
}

pub struct RemplatePath(pub String, pub proc_macro2::Span);

impl Parse for RemplatePath {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        input.parse::<kw::path>()?;
        input.parse::<Token![=]>()?;
        let path_parameter: LitStr = input.parse()?;

        Ok(Self(path_parameter.value(), path_parameter.span()))
    }
}

pub struct MacroParseResult {
    pub impl_generics: proc_macro2::TokenStream,
    pub type_generics: proc_macro2::TokenStream,
    pub where_clause: Option<proc_macro2::TokenStream>,
    pub type_ident: proc_macro2::TokenStream,
    pub template_path: RemplatePath,
}

pub fn parse_derive_macro_input(
    input: proc_macro::TokenStream,
) -> Result<MacroParseResult, syn::Error> {
    let input = syn::parse::<DeriveInput>(input)?;
    let input_span = input.span();
    let impl_type = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let template_path = match input.attrs.into_iter().find(|attr| {
        let attr_path = attr.meta.path();
        attr_path.is_ident("remplate")
    }) {
        Some(attr) => attr
            .meta
            .require_list()
            .map(|meta_list| meta_list.tokens.clone())
            .and_then(syn::parse2::<RemplatePath>)?,
        None => Err(syn::parse::Error::new(input_span, "Missing template path"))?,
    };

    Ok(MacroParseResult {
        impl_generics: impl_generics.to_token_stream(),
        type_generics: ty_generics.to_token_stream(),
        where_clause: where_clause.map(|where_clause| where_clause.to_token_stream()),
        type_ident: impl_type.to_token_stream(),
        template_path,
    })
}
