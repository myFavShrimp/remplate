#[derive(Debug)]
pub struct TemplateError<'a>(pub usize, pub &'a str);

impl<'a> TemplateError<'a> {
    const TEMPLATE_POINTER_PADDING: usize = 10;

    pub fn abortion_error(self) -> proc_macro2::TokenStream {
        let left_slice = self.left_erroneous_slice();
        let right_slice = self.right_erroneous_slice();
        let erroneous_character = self.erroneous_character();

        let TemplateError(erroneous_character_position, _) = self;

        let allowed_erroneous_slice_length =
            std::cmp::min(left_slice.len() - 1, right_slice.len() - 1);

        let final_left_slice = &left_slice
            [(left_slice.len() - allowed_erroneous_slice_length)..(left_slice.len() - 1)];

        let final_right_slice = &right_slice[1..allowed_erroneous_slice_length];

        let erroneous_slice = [final_left_slice, &erroneous_character, final_right_slice].concat();

        let mut pointer: String = (0..allowed_erroneous_slice_length).map(|_| " ").collect();
        pointer.push('^');

        let error_message = format!(
            "Failed to find closing token for `{}` at position {}:\n\"{}\"\n{}",
            erroneous_character, erroneous_character_position, erroneous_slice, pointer
        );

        quote::quote! {
            ::core::compile_error!(#error_message);
        }
    }

    fn left_erroneous_slice(&self) -> String {
        let TemplateError(erroneous_character_position, template) = self;
        let slice_start = erroneous_character_position
            .checked_sub(Self::TEMPLATE_POINTER_PADDING)
            .unwrap_or(0);
        format!(
            "{:?}",
            &template[slice_start..*erroneous_character_position]
        )
    }

    fn right_erroneous_slice(&self) -> String {
        let TemplateError(erroneous_character_position, template) = self;
        let slice_end = std::cmp::min(
            erroneous_character_position + Self::TEMPLATE_POINTER_PADDING,
            template.len(),
        );
        format!(
            "{:?}",
            &template[(*erroneous_character_position + 1)..slice_end]
        )
    }

    fn erroneous_character(&self) -> String {
        let TemplateError(erroneous_character_position, template) = self;
        template[*erroneous_character_position..(*erroneous_character_position + 1)].to_string()
    }
}
