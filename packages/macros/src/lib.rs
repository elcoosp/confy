use parse::Parser;
use proc_macro::TokenStream;
use proc_macro_error2::*;
use quote::*;
use syn::visit_mut::VisitMut;
use syn::*;
use telety::*;
#[proc_macro_error]
#[proc_macro]
pub fn mix(tokens: TokenStream) -> TokenStream {
    let args_parsed = punctuated::Punctuated::<Path, Token![,]>::parse_separated_nonempty
        .parse(tokens)
        .unwrap(); // Better to turn it into a `compile_error!()`
                   // Split `tokens` to `path_to_struct0`, `path_to_struct1`, & `new_struct_ident`
                   // ...
                   // Take the relative paths `path_to_struct0` and `path_to_struct1`
                   // and use v1::TY::apply to call mix_impl! with the actual definition
    let item0: Path = args_parsed.first().unwrap().clone();
    let item1: Path = args_parsed.get(1).unwrap().clone();

    let new_struct_ident = parse2::<Ident>(quote! {Mixed}).unwrap();
    // telety works by find and replace - define a 'needle', and put it
    // where you want the type information inserted.
    let needle0: Ident = parse_quote!(item0_goes_here);
    let needle1: Ident = parse_quote!(item1_goes_here);
    // This macro generates the call to our actual implementation.
    // The `TY.apply` calls will replace the needles with the type definitions.
    let output = quote! {
        ::confy_macros::mix_impl!{ #needle0; #needle1 }
    };

    let output = v1::TY.apply(item0, needle0, output);
    let output = v1::TY.apply(item1, needle1, output);
    output.into_token_stream().into()
}

#[proc_macro_error]
#[proc_macro]
pub fn mix_impl(tokens: TokenStream) -> TokenStream {
    // Parse macro arguments
    // ...
    let args_parsed = punctuated::Punctuated::<Item, Token![;]>::parse_separated_nonempty
        .parse(tokens)
        .unwrap();
    let item0 = args_parsed.first().unwrap().clone();
    let item1 = args_parsed.get(1).unwrap().clone();

    // Telety lets us reference remote types
    let telety0 = Telety::new(&item0).unwrap();
    let telety1 = Telety::new(&item1).unwrap();

    match item0.clone() {
        Item::Struct(ItemStruct { mut fields, .. }) => {
            for field in fields.iter_mut() {
                // We can get a location-independent alias for any type
                // used in the original item definition.
                let aliased_ty = telety0.alias_of(&field.ty).unwrap();
                // emit_call_site_error!("{aliased_ty:#?}");
                // Switch to `crate::...` if in the same crate the alias was defined,
                // otherwise keep the path as `::my_crate::...`.
                visitor::Crateify::new().visit_type_mut(&mut aliased_ty.ty());
                field.ty = aliased_ty.ty();
            }
        }
        received => {
            emit_call_site_error!(format!("Expected struct, received: {received:#?}"))
        }
    };
    // Get the fields from the struct definitions
    // ...
    // Change the original type tokens to our aliases

    // for field in fields1.iter_mut() {
    //     let mut aliased_ty = telety1.alias_of(&field.ty).unwrap();
    //     telety::visitor::Crateify::new().visit_type_mut(&mut aliased_ty);
    //     field.ty = aliased_ty;
    // }

    let new_struct_ident = parse2::<Ident>(quote! {Mixed}).unwrap();

    // Create a new struct with all the fields from both mixed types
    quote::quote! {
        pub struct #new_struct_ident {
            // #fields0
            // #fields1
        }
    }
    .into_token_stream()
    .into()
}
