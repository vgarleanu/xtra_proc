extern crate proc_macro;

use proc_macro::TokenStream;

use quote::format_ident;
use quote::quote;

use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse::Result;
use syn::*;

enum Item {
    Struct(ItemStruct),
    Impl(ItemImpl),
}

impl Parse for Item {
    fn parse(input: ParseStream) -> Result<Self> {
        let lookahead = input.lookahead1();

        Ok(if lookahead.peek(Token![impl]) {
            let item: ItemImpl = input.parse()?;
            Item::Impl(item)
        } else {
            let item: ItemStruct = input.parse()?;
            Item::Struct(item)
        })
    }
}

#[proc_macro_attribute]
pub fn actor(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as Item);
    TokenStream::from(match input {
        Item::Struct(x) => actor_struct(x),
        Item::Impl(x) => actor_impl(x),
    })
}

#[proc_macro_attribute]
pub fn handler(_: TokenStream, input: TokenStream) -> TokenStream {
    input
}

fn actor_struct(item_struct: ItemStruct) -> proc_macro2::TokenStream {
    let ItemStruct {
        attrs,
        ident,
        generics,
        fields,
        semi_token,
        ..
    } = item_struct;

    let actor_mod = format_ident!("__Actor{}", ident);

    quote! {
        #[doc(hidden)]
        #[allow(non_snake_case)]
        mod #actor_mod {
            use super::*;
            #(#attrs)*
            pub struct #ident #generics #fields #semi_token

            impl ::xtra::Actor for #ident {}
        }
    }
}

fn actor_impl(item_impl: ItemImpl) -> proc_macro2::TokenStream {
    let name = get_name(&item_impl);
    let mod_name = format_ident!("__Actor{}", name);

    let non_handler_methods = item_impl
        .items
        .iter()
        .filter(handler_function)
        .collect::<Vec<_>>();

    let handler_methods = item_impl
        .items
        .iter()
        .filter(|x| !handler_function(x))
        .collect::<Vec<_>>();

    let method_new = non_handler_methods.iter().find(|x| {
        if let ImplItem::Method(x) = x {
            if x.sig.ident.to_string() == "new" {
                return true;
            }
        }
        false
    });

    let method_new = match method_new {
        Some(ImplItem::Method(x)) => x,
        _ => panic!("Actor must have a `new` method"),
    };

    let args = method_new
        .sig
        .inputs
        .iter()
        .filter(|x| matches!(x, FnArg::Typed(_)))
        .collect::<Vec<_>>();

    let arglist = args.iter().map(|x| match x {
        FnArg::Typed(x) => x.pat.clone(),
        _ => unreachable!(),
    });

    let actor_creator = quote! {
        pub fn new<S: ::xtra::spawn::Spawner>(spawner: &mut S, #(#args),*) -> Self {
            use ::xtra::Actor;

            Self {
                addr: #mod_name::#name::new(#(#arglist),*).create(None).spawn(spawner),
            }
        }
    };

    let message_structs = generate_message_structs(&name, handler_methods.clone());
    let handlers = generate_handlers(&name, handler_methods.clone());
    let api_methods = generate_api_methods(&name, handler_methods.clone());

    quote! {
        impl #mod_name::#name {
            #(#non_handler_methods)*

            #(#handler_methods)*
        }

        #[derive(Clone)]
        pub struct #name {
            addr: ::xtra::Address<#mod_name::#name>,
        }

        #message_structs
        #handlers

        impl #name {
            #actor_creator

            #(#api_methods)*
        }
    }
}

fn generate_api_methods(
    actor_name: &Ident,
    items: Vec<&ImplItem>,
) -> Vec<proc_macro2::TokenStream> {
    items
        .iter()
        .map(|x| generate_api_method(actor_name, x))
        .collect::<Vec<_>>()
}

fn generate_api_method(actor_name: &Ident, item: &ImplItem) -> proc_macro2::TokenStream {
    let item = match item {
        ImplItem::Method(x) => x,
        _ => panic!("tried to generate struct for non fn handler"),
    };

    let fn_name = item.sig.ident.clone();
    let msg_name = format_ident!("__{}__{}", actor_name, fn_name);

    let args = item
        .sig
        .inputs
        .iter()
        .filter(|x| matches!(x, FnArg::Typed(_)))
        .collect::<Vec<_>>();

    let ret_type = &item.sig.output;

    let arglist = args.iter().map(|x| match x {
        FnArg::Typed(x) => x.pat.clone(),
        _ => unreachable!(),
    });

    quote! {
        pub async fn #fn_name(&self, #(#args),*) #ret_type {
            self.addr.send(#msg_name {
                #(#arglist),*
            }).await.expect("Actor has died.")
        }
    }
}

fn generate_message_structs(actor_name: &Ident, items: Vec<&ImplItem>) -> proc_macro2::TokenStream {
    let msg_structs = items
        .iter()
        .map(|x| generate_msg_struct(actor_name, x))
        .collect::<Vec<_>>();

    quote! {
        #(#msg_structs)*
    }
}

fn generate_msg_struct(actor_name: &Ident, item: &ImplItem) -> proc_macro2::TokenStream {
    let item = match item {
        ImplItem::Method(x) => x,
        _ => panic!("tried to generate struct for non fn handler"),
    };

    let fn_name = item.sig.ident.clone();
    let msg_name = format_ident!("__{}__{}", actor_name, fn_name);

    let args = item
        .sig
        .inputs
        .iter()
        .filter(|x| matches!(x, FnArg::Typed(_)))
        .collect::<Vec<_>>();

    let xtra_msg_impl = match &item.sig.output {
        ReturnType::Default => quote! {
            impl ::xtra::Message for #msg_name {
                type Result = ();
            }
        },
        ReturnType::Type(_, t) => quote! {
            impl ::xtra::Message for #msg_name {
                type Result = #t;
            }
        },
    };

    quote! {
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        struct #msg_name {
            #(#args),*
        }

        #[doc(hidden)]
        #[allow(non_snake_case)]
        #xtra_msg_impl
    }
}

fn generate_handlers(actor_name: &Ident, items: Vec<&ImplItem>) -> proc_macro2::TokenStream {
    let handlers = items
        .iter()
        .map(|x| generate_handler(actor_name, x))
        .collect::<Vec<_>>();

    quote! {
        #(#handlers)*
    }
}

fn generate_handler(actor_name: &Ident, item: &ImplItem) -> proc_macro2::TokenStream {
    let item = match item {
        ImplItem::Method(x) => x,
        _ => panic!("tried to generate struct for non fn handler"),
    };

    let fn_name = item.sig.ident.clone();
    let msg_name = format_ident!("__{}__{}", actor_name, fn_name);
    let mod_name = format_ident!("__Actor{}", actor_name);

    let args = item
        .sig
        .inputs
        .iter()
        .filter(|x| matches!(x, FnArg::Typed(_)))
        .collect::<Vec<_>>();

    let arglist = args
        .iter()
        .map(|x| match x {
            FnArg::Typed(x) => x.pat.clone(),
            _ => unreachable!(),
        })
        .map(|x| {
            quote! {
                args.#x
            }
        });

    quote! {
        #[async_trait]
        impl ::xtra::Handler<#msg_name> for #mod_name::#actor_name {
            async fn handle(&mut self, args: #msg_name, _: &mut ::xtra::Context<Self>)
                -> <#msg_name as ::xtra::Message>::Result
            {
                self.#fn_name(#(#arglist),*).await
            }
        }
    }
}

fn get_name(block: &ItemImpl) -> proc_macro2::Ident {
    let self_ty_path = match &*block.self_ty {
        Type::Path(path) => &path.path,
        _ => panic!(),
    };

    self_ty_path.segments.last().unwrap().ident.clone()
}

fn handler_function(x: &&ImplItem) -> bool {
    if let ImplItem::Method(x) = x {
        let ident = x
            .attrs
            .iter()
            .map(|x| x.path.segments.last().unwrap().ident.to_string())
            .collect::<Vec<_>>();

        if ident.contains(&"handler".to_string()) {
            return false;
        }
    }

    true
}
