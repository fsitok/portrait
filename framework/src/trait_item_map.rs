extern crate proc_macro;

use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Parse;
use syn::{Error, Result};

use crate::{filler, Completer};

/// One-line wrapper that declares a filler macro.
///
/// # Example
/// ```
/// # extern crate proc_macro;
/// #
/// portrait_framework::proc_macro_filler!(foo, MyGenerator);
/// struct MyGenerator(portrait_framework::NoArgs);
/// impl portrait_framework::Generator for MyGenerator {
///     fn generate_const(&mut self, item: &syn::TraitItemConst) -> Result<syn::ImplItemConst> {
///         todo!()
///     }
///     fn generate_method(&mut self, item: &syn::TraitItemMethod) -> Result<syn::ImplItemMethod> {
///         todo!()
///     }
///     fn generate_type(&mut self, item: &syn::TraitItemType) -> Result<syn::ImplItemType> {
///         todo!()
///     }
/// }
/// ```
///
/// This declares a filler macro called `foo`,
/// where each missing item is generated by calling the corresponding funciton.
#[macro_export]
macro_rules! proc_macro_filler {
    ($ident:ident, $generator:path) => {
        pub fn $ident(input: ::proc_macro::TokenStream) -> ::proc_macro::TokenStream {
            portrait_framework::completer_filler(input, $generator)
        }
    };
}

/// Shorthand from [`fn@filler`] to [`complete`] ([`proc_macro`] version).
pub fn completer_filler<ArgsT: Parse, GeneratorT: Generator>(
    input: proc_macro::TokenStream,
    ctor: fn(ArgsT) -> GeneratorT,
) -> proc_macro::TokenStream {
    completer_filler2(input.into(), ctor).unwrap_or_else(syn::Error::into_compile_error).into()
}

/// Shorthand from [`fn@filler`] to [`complete`] ([`proc_macro2`] version).
pub fn completer_filler2<ArgsT: Parse, GeneratorT: Generator>(
    input: TokenStream,
    ctor: fn(ArgsT) -> GeneratorT,
) -> Result<TokenStream> {
    struct CompleterImpl<GeneratorT, ArgsT>(fn(ArgsT) -> GeneratorT);

    impl<GeneratorT: Generator, ArgsT: Parse> Completer for CompleterImpl<GeneratorT, ArgsT> {
        type Args = ArgsT;

        fn complete(
            self,
            portrait: &[syn::TraitItem],
            args: Self::Args,
            item_impl: &syn::ItemImpl,
        ) -> Result<TokenStream> {
            let tokens = complete(portrait, item_impl, self.0(args))?;
            Ok(quote!(#tokens))
        }
    }

    filler(input, CompleterImpl(ctor))
}

/// Invokes the generator on each unimplemented item
/// and returns a clone of `impl_block` with the generated items.
pub fn complete(
    trait_items: &[syn::TraitItem],
    impl_block: &syn::ItemImpl,
    mut generator: impl Generator,
) -> syn::Result<syn::ItemImpl> {
    let mut output = impl_block.clone();

    let items = subtract_items(trait_items, impl_block)?;
    for trait_item in items.consts.values() {
        let impl_item = generator.generate_const(trait_item)?;
        output.items.push(syn::ImplItem::Const(impl_item));
    }
    for trait_item in items.methods.values() {
        let impl_item = generator.generate_method(trait_item)?;
        output.items.push(syn::ImplItem::Method(impl_item));
    }
    for trait_item in items.types.values() {
        let impl_item = generator.generate_type(trait_item)?;
        output.items.push(syn::ImplItem::Type(impl_item));
    }

    Ok(output)
}

/// Generates missing items.
pub trait Generator {
    /// Implements an associated constant.
    fn generate_const(&mut self, item: &syn::TraitItemConst) -> Result<syn::ImplItemConst>;

    /// Implements an associated function.
    fn generate_method(&mut self, item: &syn::TraitItemMethod) -> Result<syn::ImplItemMethod>;

    /// Implements an associated type.
    fn generate_type(&mut self, item: &syn::TraitItemType) -> Result<syn::ImplItemType>;
}

/// Shorthand for `TraitItemMap::new().minus(ImplItemMap::new())`.
pub fn subtract_items<'t>(
    trait_items: &'t [syn::TraitItem],
    impl_block: &'t syn::ItemImpl,
) -> syn::Result<TraitItemMap<'t>> {
    let mut items = TraitItemMap::new(trait_items);
    items.minus(&ImplItemMap::new(impl_block))?;
    Ok(items)
}

/// Indexes items in a trait by namespaced identifier.
#[derive(Default)]
pub struct TraitItemMap<'t> {
    /// Associated constants in the trait.
    pub consts:  HashMap<syn::Ident, &'t syn::TraitItemConst>,
    /// Associated functions in the trait.
    pub methods: HashMap<syn::Ident, &'t syn::TraitItemMethod>,
    /// Associated types in the trait.
    pub types:   HashMap<syn::Ident, &'t syn::TraitItemType>,
}

impl<'t> TraitItemMap<'t> {
    /// Constructs the trait item index from a slice of trait items.
    pub fn new(trait_items: &'t [syn::TraitItem]) -> Self {
        let mut map = Self::default();
        for item in trait_items {
            match item {
                syn::TraitItem::Const(item) => {
                    map.consts.insert(item.ident.clone(), item);
                }
                syn::TraitItem::Method(item) => {
                    map.methods.insert(item.sig.ident.clone(), item);
                }
                syn::TraitItem::Type(item) => {
                    map.types.insert(item.ident.clone(), item);
                }
                _ => {}
            }
        }
        map
    }

    /// Removes the items found in the impl, leaving only unimplemented items.
    pub fn minus(&mut self, impl_items: &ImplItemMap) -> Result<()> {
        for (ident, impl_item) in &impl_items.consts {
            if self.consts.remove(ident).is_none() {
                return Err(Error::new_spanned(
                    impl_item,
                    "no associated constant called {ident} in trait",
                ));
            }
        }

        for (ident, impl_item) in &impl_items.methods {
            if self.methods.remove(ident).is_none() {
                return Err(Error::new_spanned(
                    impl_item,
                    "no associated function called {ident} in trait",
                ));
            }
        }

        for (ident, impl_item) in &impl_items.types {
            if self.types.remove(ident).is_none() {
                return Err(Error::new_spanned(
                    impl_item,
                    "no associated type called {ident} in trait",
                ));
            }
        }

        Ok(())
    }
}

/// Indexes items in an impl block by namespaced identifier.
#[derive(Default)]
pub struct ImplItemMap<'t> {
    /// Associated constants in the implementation.
    pub consts:  HashMap<syn::Ident, &'t syn::ImplItemConst>,
    /// Associated functions in the implementation.
    pub methods: HashMap<syn::Ident, &'t syn::ImplItemMethod>,
    /// Associated types in the implementation.
    pub types:   HashMap<syn::Ident, &'t syn::ImplItemType>,
}

impl<'t> ImplItemMap<'t> {
    /// Constructs the impl item index from an impl block.
    pub fn new(impl_block: &'t syn::ItemImpl) -> Self {
        let mut map = Self::default();
        for item in &impl_block.items {
            match item {
                syn::ImplItem::Const(item) => {
                    map.consts.insert(item.ident.clone(), item);
                }
                syn::ImplItem::Method(item) => {
                    map.methods.insert(item.sig.ident.clone(), item);
                }
                syn::ImplItem::Type(item) => {
                    map.types.insert(item.ident.clone(), item);
                }
                _ => {}
            }
        }
        map
    }
}
