use convert_case::{Case, Casing};
use proc_macro::{self, TokenStream};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::{parse_macro_input, spanned::Spanned, DeriveInput, Fields};

#[proc_macro_derive(Command)]
pub fn command(input: TokenStream) -> TokenStream {
    let DeriveInput { ident, data, .. } = parse_macro_input!(input);

    let mut run_variants = TokenStream2::new();
    let mut from_args_variants = TokenStream2::new();
    let mut lowercase_variants = vec![];
    match data {
        syn::Data::Enum(s) => {
            for variant in &s.variants {
                let ref variant_name = variant.ident;
                // Variant must be either Variant(module::Command)
                // or Variant
                let has_command = match &variant.fields {
                    Fields::Unit => false,
                    Fields::Unnamed(fields) => {
                        assert_eq!(fields.unnamed.len(), 1, "Tuple must be of length one");
                        true
                    }
                    _ => panic!("Unit or Unnamed fields required"),
                };
                let mut lowercase_variant =
                    format_ident!("{}", variant_name.to_string().to_case(Case::Snake));
                lowercase_variant.set_span(variant_name.span());
                run_variants.extend(if has_command {
                    quote_spanned! {variant.span()=>
                        #variant_name(command) => command.run(data).await,
                    }
                } else {
                    quote_spanned! {variant.span()=>
                        #variant_name => #lowercase_variant::run(data).await,
                    }
                });
                let wrapped_string = lowercase_variant.to_string();
                from_args_variants.extend(if has_command {
                    quote_spanned! {variant.span()=>
                        #wrapped_string => Ok(#ident::#variant_name(#lowercase_variant::Command::from_args(args).context(#wrapped_string)?)),
                    }
                } else {
                    quote_spanned! {variant.span()=>
                        #wrapped_string => Ok(#ident::#variant_name),
                    }
                });
                lowercase_variants.push(lowercase_variant.to_string());
            }
        }
        _ => {
            panic!("Deriving Command requires an enum");
        }
    };

    let expected_message = format!(
        "Expected one of the following: [{}]",
        lowercase_variants.join(", ")
    );

    let output = quote! {
        use ::serde_json::Value;
        use ::anyhow::{anyhow, Context, Result};
        impl #ident {
            pub async fn run(self, data: Value) -> Result<Value, Value> {
                use #ident::*;
                match self {
                    #run_variants
                }
            }
            pub fn from_args(args: &[String]) -> Result<Command> {
                let (first_arg, args) = args.split_first().ok_or(anyhow!(#expected_message))?;
                match first_arg.as_str() {
                    #from_args_variants
                    _ => Err(anyhow!(
                        #expected_message
                    )),
                }
            }
        }
    };
    output.into()
}
