use proc_macro::TokenStream;
use quote::quote;
use syn::{
    bracketed, parenthesized,
    parse::Parse,
    parse_macro_input,
    punctuated::Punctuated,
    token::{Bracket, Paren},
    Ident, LitStr, Token,
};

struct FullBnf {
    lines: Vec<BnfLine>,
}

impl Parse for FullBnf {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut lines = Vec::new();

        while !input.is_empty() {
            let line = input.parse()?;
            let _: Token![;] = input.parse()?;
            lines.push(line);
        }

        Ok(Self { lines })
    }
}

enum BnfLine {
    Tag(BnfTag),
    Enum(BnfEnum),
}

impl Parse for BnfLine {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.peek(Token![enum]) {
            let tag = input.parse()?;
            Ok(Self::Enum(tag))
        } else {
            let tag = input.parse()?;
            Ok(Self::Tag(tag))
        }
    }
}

struct BnfTag {
    name: Ident,
    args: BnfTagArgs,
}

impl Parse for BnfTag {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;

        let _: Token![=] = input.parse()?;

        let args = input.parse()?;

        Ok(Self { name, args })
    }
}

impl BnfTag {
    fn as_parse_quote(&self, input: &Ident) -> proc_macro2::TokenStream {
        let tagname = &self.name;
        let parse = self.args.args.iter().map(|arg| {
            if let Some(name) = &arg.name {
                let parse_kind = arg.parse_kind.as_parse_quote(input);
                quote! {
                    let #name = { #parse_kind }
                        .map_err(|err| ParseError::with_child(
                            concat!("Failed to parse ", stringify!(#tagname)),
                            #input.pos,
                            err,
                        ))?;
                }
            } else {
                let parse_kind = arg.parse_kind.as_parse_quote(input);
                quote! {
                    { #parse_kind }
                        .map_err(|err| ParseError::with_child(
                            concat!("Failed to parse ", stringify!(#tagname)),
                            #input.pos,
                            err,
                        ))?;
                }
            }
        });

        let assign = self.args.args.iter().map(|arg| {
            if let Some(name) = &arg.name {
                quote! { #name, }
            } else {
                quote! {}
            }
        });

        let name = &self.name;

        quote! { {
            #(#parse)*
            #name { #(#assign)* __lt: Default::default() }
        } }
    }
}

struct BnfEnum {
    name: Ident,
    args: Vec<Ident>,
}

impl BnfEnum {
    fn as_def_quote(&self) -> proc_macro2::TokenStream {
        let args = &self.args;

        let args = args.iter().map(|arg| {
            quote! { #arg(#arg<'a>) }
        });

        quote! {
            {
                #(#args),*
            }
        }
    }

    fn as_parse_quote(&self, input: &Ident) -> proc_macro2::TokenStream {
        let args = &self.args;
        let errors = Ident::new("errors", self.name.span());

        let args = args.iter().map(|arg| {
            quote! {
                let parsed = #arg::parse(#input);
                match parsed {
                    Ok((parsed, input)) => {
                        return Ok((Self::#arg(parsed), input))
                    }
                    Err(e) => {
                        #errors.push(e);
                    }
                }
            }
        });

        let name = &self.name;

        quote! {
            let mut #errors = Vec::new();
            #(#args)*
            Err(ParseError::with_children(concat!("Couldn't parse ", stringify!(#name)), #input.pos, #errors))
        }
    }
}

impl Parse for BnfEnum {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let _: Token![enum] = input.parse()?;

        let name = input.parse()?;

        let _: Token![=] = input.parse()?;

        // Parse idents punctuated with | until ;
        let content;
        let _ = bracketed!(content in input);
        let idents: Punctuated<Ident, Token![|]> = content.parse_terminated(Ident::parse)?;

        Ok(Self {
            name,
            args: idents.into_iter().collect(),
        })
    }
}

struct BnfTagArgs {
    args: Vec<BnfTagNamedArg>,
}

impl BnfTagArgs {
    fn as_def_quote(&self) -> proc_macro2::TokenStream {
        let args = self.args.iter().map(|arg| {
            if let Some(name) = arg.name.as_ref() {
                let parse_kind = arg.parse_kind.as_def_quote();
                quote! { pub #name: #parse_kind, }
            } else {
                quote! {}
            }
        });

        quote! { { #(#args)* __lt: std::marker::PhantomData<&'a ()> } }
    }
}

impl Parse for BnfTagArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut args = Vec::new();
        while !input.peek(Token![;]) {
            args.push(input.parse()?);
        }

        Ok(Self { args })
    }
}

struct BnfTagNamedArg {
    // is none when ident is _
    name: Option<Ident>,
    parse_kind: BnfTagArgKind,
}

impl Parse for BnfTagNamedArg {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name = if input.peek(Ident) && input.peek2(Token![:]) {
            let name = input.parse()?;
            let _: Token![:] = input.parse()?;
            Some(name)
        } else {
            None
        };

        let parse_kind = input.parse()?;

        Ok(Self { name, parse_kind })
    }
}

enum BnfTermArgModifier {
    Box,
    Vec,
    VecUntilEof,
    Lookahead,
    Not,
    Optional,
}

enum BnfTagArgKind {
    Term {
        name: Ident,
        modifier: Option<BnfTermArgModifier>,
    },
    String(LitStr),
    Regex(LitStr),
    CustomFn(Ident),
    Eof,
}

impl BnfTagArgKind {
    fn as_def_quote(&self) -> proc_macro2::TokenStream {
        match self {
            Self::Term { name, modifier } => match modifier {
                Some(BnfTermArgModifier::Box) => {
                    quote! { Box<#name<'a>> }
                }
                Some(BnfTermArgModifier::Vec) => {
                    quote! { Vec<#name<'a>> }
                }
                Some(BnfTermArgModifier::VecUntilEof) => {
                    quote! { Vec<#name<'a>> }
                }
                Some(BnfTermArgModifier::Optional) => {
                    quote! { Option<#name<'a>> }
                }
                Some(BnfTermArgModifier::Lookahead) => {
                    quote! { () }
                }
                Some(BnfTermArgModifier::Not) => {
                    quote! { () }
                }
                None => {
                    quote! { #name<'a> }
                }
            },
            Self::String(_) => {
                quote! { TextLit<'a> }
            }
            Self::Regex(_) => {
                quote! { TextLit<'a> }
            }
            Self::CustomFn(_) => {
                quote! { TextLit<'a> }
            }
            Self::Eof => {
                quote! { () }
            }
        }
    }

    fn as_parse_quote(&self, input: &Ident) -> proc_macro2::TokenStream {
        let inner = match self {
            Self::Term { name, modifier } => match modifier {
                Some(BnfTermArgModifier::Box) => {
                    quote! {
                        match #name::parse(#input) {
                            Ok((result, rest)) => Ok((Box::new(result), rest)),
                            Err(err) => Err(err),
                        }
                    }
                }
                Some(BnfTermArgModifier::Vec) => {
                    quote! { {
                        let mut inp = #input;
                        let mut vec = Vec::new();
                        while let Ok((result, rest)) = #name::parse(inp) {
                            vec.push(result);
                            inp = rest;
                        }
                        Ok((vec, inp))
                    } }
                }
                Some(BnfTermArgModifier::VecUntilEof) => {
                    quote! { {
                        let mut inp = #input;
                        let mut vec = Vec::new();
                        loop {
                            match #name::parse(inp) {
                                Ok((result, rest)) => {
                                    vec.push(result);
                                    inp = rest;
                                }
                                Err(err) => {
                                    if inp.is_empty() {
                                        break;
                                    } else {
                                        return Err(err);
                                    }
                                },
                            }
                        }
                        Ok((vec, inp))
                    } }
                }
                Some(BnfTermArgModifier::Optional) => {
                    quote! {
                        match #name::parse(#input) {
                            Ok((result, rest)) => Ok((Some(result), rest)),
                            Err(err) => (Ok((None, #input))),
                        }
                    }
                }
                Some(BnfTermArgModifier::Lookahead) => {
                    quote! { {
                        match #name::parse(#input) {
                            Ok((result, rest)) => Ok(((), #input)),
                            Err(err) => Err(err),
                        }
                    } }
                }
                Some(BnfTermArgModifier::Not) => {
                    quote! { {
                        match #name::parse(#input) {
                            Ok((result, rest)) => Err(ParseError::new(concat!(stringify!(#name), " not allowed here"), #input.pos)),
                            Err(err) => Ok(((), #input)),
                        }
                    } }
                }
                None => {
                    quote! { #name::parse(#input) }
                }
            },
            Self::String(lit) => {
                quote! {
                    parse_string_lit(#input, #lit)
                        .map_err(|_| ParseError::new(concat!("Expected \"", #lit, "\""), #input.pos))
                }
            }
            Self::Regex(lit) => {
                quote! {
                    parse_string_regex(#input, #lit)
                        .map_err(|_| ParseError::new(concat!("Expected regex \"", #lit, "\""), #input.pos))
                }
            }
            Self::CustomFn(ident) => {
                quote! {
                    #ident(#input)
                }
            }
            Self::Eof => {
                quote! {
                    if #input.is_empty() {
                        Ok(((), #input))
                    } else {
                        Err(ParseError::new("Expected end of file", #input.pos))
                    }
                }
            }
        };

        quote! {
            {
                let r = #inner;
                match r {
                    Ok((result, rest)) => {
                        #input = rest;
                        Ok(result)
                    }
                    Err(err) => Err(err),
                }
            }
        }
    }
}

impl Parse for BnfTagArgKind {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.peek(Token![<]) {
            let _: Token![<] = input.parse()?;

            let name: Ident;
            let modifier: Option<BnfTermArgModifier>;

            if input.peek(Bracket) {
                let content;
                let _ = bracketed!(content in input);

                name = content.parse()?;

                if input.peek(Token![^]) {
                    let _: Token![^] = input.parse()?;
                    modifier = Some(BnfTermArgModifier::VecUntilEof)
                } else {
                    modifier = Some(BnfTermArgModifier::Vec)
                }
            } else if input.peek(Paren) {
                let content;
                let _ = parenthesized!(content in input);

                name = content.parse()?;
                modifier = Some(BnfTermArgModifier::Lookahead)
            } else if input.peek(Token![*]) {
                let _: Token![*] = input.parse()?;

                name = input.parse()?;
                modifier = Some(BnfTermArgModifier::Box)
            } else if input.peek(Token![?]) {
                let _: Token![?] = input.parse()?;

                name = input.parse()?;
                modifier = Some(BnfTermArgModifier::Optional)
            } else if input.peek(Token![!]) {
                let _: Token![!] = input.parse()?;

                name = input.parse()?;
                modifier = Some(BnfTermArgModifier::Not)
            } else {
                name = input.parse()?;
                modifier = None;
            }

            let _: Token![>] = input.parse()?;
            Ok(Self::Term { name, modifier })
        } else if input.peek(Paren) {
            let content;
            let _ = parenthesized!(content in input);

            let ident = content.parse()?;
            Ok(Self::CustomFn(ident))
        } else if input.peek(LitStr) {
            let string: LitStr = input.parse()?;

            Ok(Self::String(string))
        } else if input.peek(Token![#]) && input.peek2(LitStr) {
            let _: Token![#] = input.parse()?;
            let regex: LitStr = input.parse()?;

            Ok(Self::Regex(regex))
        } else if input.peek(Token![^]) {
            let _: Token![^] = input.parse()?;

            Ok(Self::Eof)
        } else {
            Err(syn::Error::new(input.span(), "invalid argument"))
        }
    }
}

#[proc_macro]
pub fn bnf(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a syntax tree
    let input = parse_macro_input!(input as FullBnf);

    let mut definitions = Vec::new();

    for line in input.lines {
        let definition = match &line {
            BnfLine::Tag(tag) => {
                let name = &tag.name;
                let args_quote = tag.args.as_def_quote();
                quote! {
                    #[derive(Debug, Clone, Hash, PartialEq)]
                    pub struct #name <'a> #args_quote
                }
            }
            BnfLine::Enum(en) => {
                let name = &en.name;
                let args_quote = en.as_def_quote();
                quote! {
                    #[derive(Debug, Clone, Hash, PartialEq)]
                    pub enum #name <'a> #args_quote
                }
            }
        };

        definitions.push(definition);

        let input_ident = Ident::new("input", proc_macro2::Span::call_site());
        let definition = match &line {
            BnfLine::Tag(tag) => {
                let name = &tag.name;
                let parse_quote = tag.as_parse_quote(&input_ident);
                quote! {
                    impl<'a> #name<'a> {
                        pub fn parse(mut #input_ident: StringParser<'a>) -> Result<(Self, StringParser<'a>), ParseError> {
                            Ok((#parse_quote, #input_ident))
                        }
                    }
                }
            }
            BnfLine::Enum(en) => {
                let name = &en.name;
                let parse_quote = en.as_parse_quote(&input_ident);
                quote! {
                    impl<'a> #name<'a> {
                        pub fn parse(mut #input_ident: StringParser<'a>) -> Result<(Self, StringParser<'a>), ParseError> {
                            #parse_quote
                        }
                    }
                }
            }
        };

        definitions.push(definition);
    }

    let extras = quote! {};

    let expanded = quote! {
        #extras
        #(#definitions)*
    };

    // eprintln!("\n\n{}\n\n", &expanded.to_string());

    TokenStream::from(expanded)
}
