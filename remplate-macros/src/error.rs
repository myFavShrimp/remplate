use std::{ops::Range, path::PathBuf};

#[derive(Debug)]
pub enum TemplateErrorKind {
    ClosingToken,
    MissingValue,
}

impl std::fmt::Display for TemplateErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            TemplateErrorKind::ClosingToken => "Failed to find closing token for",
            TemplateErrorKind::MissingValue => "The format expression misses a value -",
        })
    }
}

#[derive(Debug)]
pub struct TemplateError<'a>(
    pub Range<usize>,
    pub &'a PathBuf,
    pub &'a str,
    pub TemplateErrorKind,
);

impl<'a> TemplateError<'a> {
    const TEMPLATE_POINTER_PADDING: usize = 20;

    pub fn abortion_error(self) -> proc_macro2::TokenStream {
        let left_slice = self.left_erroneous_slice();
        let right_slice = self.right_erroneous_slice();
        let erroneous_character = self.erroneous_character();

        let TemplateError(erroneous_character_position, path, _, error_kind) = self;

        let allowed_erroneous_slice_length =
            std::cmp::min(left_slice.len() - 1, right_slice.len() - 1);

        let final_left_slice = &left_slice
            [(left_slice.len() - allowed_erroneous_slice_length)..(left_slice.len() - 1)];

        let final_right_slice = &right_slice[1..allowed_erroneous_slice_length];

        let erroneous_slice = [final_left_slice, &erroneous_character, final_right_slice].concat();

        let mut pointer: String = (0..allowed_erroneous_slice_length).map(|_| " ").collect();
        pointer.push('^');

        let error_message = format!(
            "{} `{}` at position {:?} in template {:?}:\n\"{}\"\n{}",
            error_kind,
            erroneous_character,
            erroneous_character_position,
            path,
            erroneous_slice,
            pointer
        );

        quote::quote! {
            ::core::compile_error!(#error_message);
        }
    }

    fn left_erroneous_slice(&self) -> String {
        let TemplateError(erroneous_character_position, _, template, _) = self;
        let slice_start = erroneous_character_position
            .start
            .checked_sub(Self::TEMPLATE_POINTER_PADDING)
            .unwrap_or(0);
        format!(
            "{:?}",
            &template[slice_start..erroneous_character_position.start]
        )
    }

    fn right_erroneous_slice(&self) -> String {
        let TemplateError(erroneous_character_position, _, template, _) = self;
        let slice_end = std::cmp::min(
            erroneous_character_position.end + Self::TEMPLATE_POINTER_PADDING,
            template.len(),
        );
        format!(
            "{:?}",
            &template[(erroneous_character_position.end)..slice_end]
        )
    }

    fn erroneous_character(&self) -> String {
        let TemplateError(erroneous_character_position, _, template, _) = self;
        let erroneous_character = format!("{:?}", &template[erroneous_character_position.clone()]);
        erroneous_character[1..(erroneous_character.len() - 1)].to_string()
    }
}
