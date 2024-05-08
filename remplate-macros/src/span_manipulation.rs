pub fn set_span_for_token_stream(
    token_stream: proc_macro2::TokenStream,
    span: proc_macro2::Span,
) -> proc_macro2::TokenStream {
    let token_tree_iter = token_stream.into_iter();

    token_tree_iter
        .map(|token_tree| set_span_for_token_tree(token_tree, span))
        .collect()
}

fn set_span_for_token_tree(
    mut token_tree: proc_macro2::TokenTree,
    span: proc_macro2::Span,
) -> proc_macro2::TokenTree {
    match &mut token_tree {
        proc_macro2::TokenTree::Group(group) => {
            let delimiter = group.delimiter();
            let inner = set_span_for_token_stream(group.stream(), span);

            let mut new_group = proc_macro2::Group::new(delimiter, inner);
            new_group.set_span(span);

            *group = new_group;
        }
        proc_macro2::TokenTree::Ident(ident) => {
            ident.set_span(span);
        }
        proc_macro2::TokenTree::Punct(punct) => {
            punct.set_span(span);
        }
        proc_macro2::TokenTree::Literal(literal) => {
            literal.set_span(span);
        }
    };

    token_tree
}
