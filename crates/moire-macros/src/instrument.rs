use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::{quote, quote_spanned};
use unsynn::{
    BraceGroup, BracketGroup, IParse, Ident, ParenthesisGroup, Pound, ToTokens as UnsynnToTokens,
    TokenIter, TokenStreamUntil, keyword, unsynn,
};

keyword! {
    KFn = "fn";
}

unsynn! {
    struct Attribute(Pound, BracketGroup);
}

unsynn! {
    struct FnSignature {
        attributes: Vec<Attribute>,
        pre_fn: TokenStreamUntil<KFn>,
        _fn_kw: KFn,
        name: Ident,
        pre_params: TokenStreamUntil<ParenthesisGroup>,
        params: ParenthesisGroup,
        tail: TokenStreamUntil<BraceGroup>,
        body: BraceGroup,
    }
}

struct SplitTail {
    return_type: Option<TokenStream2>,
    where_clause: TokenStream2,
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand_impl(attr.into(), item.into()).into()
}

fn expand_impl(attr: TokenStream2, item: TokenStream2) -> TokenStream2 {
    if !attr.is_empty() {
        return compile_error(
            Span::call_site(),
            "`#[moire::instrument]` does not accept attribute arguments",
        );
    }

    let parsed = match parse_signature(&item) {
        Ok(parsed) => parsed,
        Err(message) => return compile_error(Span::call_site(), &message),
    };

    let FnSignature {
        attributes,
        pre_fn,
        _fn_kw: _,
        name,
        pre_params,
        params,
        tail,
        body,
    } = parsed;

    let fn_name = function_name(&name);
    let attributes_tokens = attributes.to_token_stream();
    let params_tokens = params.to_token_stream();
    let body_tokens = body.to_token_stream();
    let pre_params_tokens = pre_params.into_inner();
    let tail_tokens = tail.into_inner();
    let (pre_fn_without_async, is_async) = strip_async_keyword(pre_fn.into_inner());

    let split_tail = match split_return_and_where(&tail_tokens) {
        Ok(split_tail) => split_tail,
        Err(message) => return compile_error(name.span(), &message),
    };

    // Use the span of the function name so backtraces point at the original fn,
    // not at the #[moire::instrument] attribute.
    let fn_span = name.span();

    if is_async {
        let output_ty = split_tail.return_type.unwrap_or_else(|| quote!(()));
        let where_clause = split_tail.where_clause;

        return quote_spanned! {
            fn_span =>
            #attributes_tokens
            #pre_fn_without_async fn #name #pre_params_tokens #params_tokens -> impl ::core::future::Future<Output = #output_ty> #where_clause {
                ::moire::__internal::instrument_future(#fn_name, async move #body_tokens, None, None)
            }
        };
    }

    let Some(return_type) = split_tail.return_type else {
        return compile_error(
            name.span(),
            "non-async `#[moire::instrument]` functions must return `impl Future<Output = T>`",
        );
    };

    if !contains_ident(&return_type, "impl") || !contains_ident(&return_type, "Future") {
        return compile_error(
            name.span(),
            "non-async `#[moire::instrument]` functions must return `impl Future<Output = T>`",
        );
    }

    quote_spanned! {
        fn_span =>
        #attributes_tokens
        #pre_fn_without_async fn #name #pre_params_tokens #params_tokens #tail_tokens {
            ::moire::__internal::instrument_future(#fn_name, #body_tokens, None, None)
        }
    }
}

fn parse_signature(item: &TokenStream2) -> Result<FnSignature, String> {
    let mut tokens = TokenIter::new(item.clone());
    (&mut tokens)
        .parse_all::<FnSignature>()
        .map_err(|err| format!("`#[moire::instrument]` expects a function item: {err}"))
}

fn strip_async_keyword(tokens: TokenStream2) -> (TokenStream2, bool) {
    let mut stripped = TokenStream2::new();
    let mut saw_async = false;

    for token in tokens {
        if let TokenTree::Ident(ident) = &token
            && ident == "async"
        {
            saw_async = true;
            continue;
        }

        stripped.extend(std::iter::once(token));
    }

    (stripped, saw_async)
}

fn split_return_and_where(tail: &TokenStream2) -> Result<SplitTail, String> {
    let tokens: Vec<TokenTree> = tail.clone().into_iter().collect();
    let where_index = tokens
        .iter()
        .position(|token| matches!(token, TokenTree::Ident(ident) if ident == "where"))
        .unwrap_or(tokens.len());

    let before_where = &tokens[..where_index];
    let where_clause = collect_tokens(&tokens[where_index..]);
    let return_type = parse_return_type(before_where)?;

    Ok(SplitTail {
        return_type,
        where_clause,
    })
}

fn parse_return_type(tokens: &[TokenTree]) -> Result<Option<TokenStream2>, String> {
    if tokens.is_empty() {
        return Ok(None);
    }

    if tokens.len() < 2 {
        return Err("invalid return type: expected `-> <type>`".to_owned());
    }

    if !is_punct(&tokens[0], '-') || !is_punct(&tokens[1], '>') {
        return Err(
            "invalid signature tail: expected `-> <type>` before the function body".to_owned(),
        );
    }

    let return_type = collect_tokens(&tokens[2..]);
    if return_type.is_empty() {
        return Err("missing return type after `->`".to_owned());
    }

    Ok(Some(return_type))
}

fn collect_tokens(tokens: &[TokenTree]) -> TokenStream2 {
    tokens.iter().cloned().collect()
}

fn is_punct(token: &TokenTree, ch: char) -> bool {
    matches!(token, TokenTree::Punct(punct) if punct.as_char() == ch)
}

fn contains_ident(tokens: &TokenStream2, needle: &str) -> bool {
    tokens.clone().into_iter().any(|token| match token {
        TokenTree::Ident(ident) => ident == needle,
        TokenTree::Group(group) => contains_ident(&group.stream(), needle),
        _ => false,
    })
}

fn function_name(name: &Ident) -> String {
    let raw = name.to_string();
    raw.strip_prefix("r#").unwrap_or(&raw).to_owned()
}

fn compile_error(span: Span, message: &str) -> TokenStream2 {
    quote_spanned! {
        span =>
        compile_error!(#message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_async_functions() {
        let input = quote! {
            pub async fn fetch_data(id: u64) -> String {
                id.to_string()
            }
        };

        let output = expand_impl(TokenStream2::new(), input);
        let expected = quote! {
            pub fn fetch_data(id: u64) -> impl ::core::future::Future<Output = String> {
                ::moire::__internal::instrument_future("fetch_data", async move {
                    id.to_string()
                }, None, None)
            }
        };

        assert_eq!(output.to_string(), expected.to_string());
    }

    #[test]
    fn rewrites_impl_future_functions() {
        let input = quote! {
            fn make_future() -> impl ::core::future::Future<Output = usize> {
                async { 42 }
            }
        };

        let output = expand_impl(TokenStream2::new(), input);
        let expected = quote! {
            fn make_future() -> impl ::core::future::Future<Output = usize> {
                ::moire::__internal::instrument_future("make_future", {
                    async { 42 }
                }, None, None)
            }
        };

        assert_eq!(output.to_string(), expected.to_string());
    }

    #[test]
    fn rejects_non_future_non_async_functions() {
        let input = quote! {
            fn plain() -> usize {
                7
            }
        };

        let output = expand_impl(TokenStream2::new(), input);
        assert!(output.to_string().contains(
            "non-async `#[moire::instrument]` functions must return `impl Future<Output = T>`"
        ));
    }
}
