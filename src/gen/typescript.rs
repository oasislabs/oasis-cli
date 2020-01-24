use heck::*;
use oasis_rpc::Interface;
use proc_macro2::{Punct, Spacing, TokenStream};
use quote::{format_ident, quote};

macro_rules! format_ts_ident {
    (@module, $fmt_str: literal, $ident:expr$(, $($args:expr),*)?) => {
        format_ident!($fmt_str, $ident.to_kebab_case(), $($args),*)
    };
    (@import, $fmt_str: literal, $ident:expr$(, $($args:expr),*)?) => {
        format_ident!($fmt_str, $ident.to_mixed_case(), $($args),*)
    };
    (@class, $fmt_str: literal, $ident:expr$(, $($args:expr),*)?) => {
        format_ident!($fmt_str, $ident.to_camel_case(), $($args),*)
    };
    (@var, $fmt_str: literal, $ident:expr$(, $($args:expr),*)?) => {
        format_ident!($fmt_str, $ident.to_mixed_case(), $($args),*)
    };
    (@const, $fmt_str: literal, $ident:expr$(, $($args:expr),*)?) => {
        format_ident!($fmt_str, $ident.to_shouty_snake_case(), $($args),*)
    };
}

pub fn generate(iface: &Interface) -> TokenStream {
    let service_ident = format_ts_ident!(@class, "{}", iface.name);

    let imports = iface.imports.iter().map(|imp| {
        let import_ident = format_ts_ident!(@class, "{}", imp.name);
        let module_name = format!(
            "./{}",
            format_ts_ident!(@module, "{}", imp.name).to_string()
        );
        quote!(import  #import_ident from #module_name)
    });

    let type_defs = generate_type_defs(&iface.type_defs);

    let rpc_functions = generate_rpc_functions(&iface.functions);

    quote! {
        import { Gateway } from "@oasislabs/gateway";
        import { Address, Balance, RpcOptions } from "@oasislabs/service";
        import { abiEncode, abiDecode, decodeHex } from "@oasislabs/oasis-std";

        #(#imports)*

        #(#type_defs)*

        export class #service_ident {
            public constructor(public address: Address, private gateway: Gateway) {}

            #(#rpc_functions)*
        }

        export default #service_ident;
    }
}

fn generate_type_defs(type_defs: &[oasis_rpc::TypeDef]) -> Vec<TokenStream> {
    type_defs
        .iter()
        .map(|type_def| {
            use oasis_rpc::TypeDef;

            match type_def {
                TypeDef::Struct { name, fields } => generate_struct_class(name, fields, quote!()),
                TypeDef::Enum { name, variants } => {
                    let type_ident = format_ts_ident!(@class, "{}", name);
                    let is_tagged_union = variants.iter().any(|v| v.fields.is_some());
                    if !is_tagged_union {
                        let field_idents = variants
                            .iter()
                            .map(|v| format_ts_ident!(@class, "{}", v.name));
                        quote! {
                            export enum #type_ident {
                                #(#field_idents),*
                            }
                        }
                    } else {
                        let (variant_classes, variant_names): (Vec<_>, Vec<_>) = variants
                            .iter()
                            .map(|variant| {
                                let variant_name =
                                    format_ts_ident!(@class, "{}", variant.name).to_string();
                                let variant_class = match &variant.fields {
                                    Some(oasis_rpc::EnumFields::Named(fields)) => {
                                        let is_tuple = fields
                                            .iter()
                                            .enumerate()
                                            .all(|(i, field)| field.name == i.to_string());
                                        if !is_tuple {
                                            generate_struct_class(
                                                &variant.name,
                                                fields.iter(),
                                                quote!(),
                                            )
                                        } else {
                                            generate_tuple_class(
                                                &variant.name,
                                                fields.iter().map(|f| &f.ty),
                                            )
                                        }
                                    }
                                    Some(oasis_rpc::EnumFields::Tuple(tys)) => {
                                        generate_tuple_class(&variant.name, tys.iter())
                                    }
                                    None => generate_struct_class(
                                        &variant.name,
                                        std::iter::empty(),
                                        quote!(public static kind = #variant_name),
                                    ),
                                };
                                (variant_class, variant_name)
                            })
                            .unzip();
                        quote! {
                            module #type_ident {
                                const VARIANTS: string[] = [ #(#variant_names),* ];

                                #(#variant_classes)*
                            }
                        }
                    }
                }
                TypeDef::Event {
                    name,
                    fields: indexed_fields,
                } => {
                    let (topic_names, topic_tys): (Vec<_>, Vec<_>) = indexed_fields
                        .iter()
                        .map(|f| (format_ts_ident!(@var, "{}", &f.name), quote_ty(&f.ty)))
                        .unzip();
                    let extra_members = quote! {
                        public async subscribe(
                            { #(#topic_names),* }: { #(#topic_names: #topic_tys),* }
                        ): Promise<void> {
                            return Promise.reject("unimplemented");
                        }
                    };
                    let fields: Vec<_> = indexed_fields
                        .iter()
                        .cloned()
                        .map(|f| oasis_rpc::Field {
                            name: f.name,
                            ty: f.ty,
                        })
                        .collect();
                    generate_struct_class(name, fields.iter(), extra_members)
                }
            }
        })
        .collect()
}

fn generate_struct_class<'a>(
    struct_name: &str,
    fields: impl IntoIterator<Item = &'a oasis_rpc::Field>,
    extra_members: TokenStream,
) -> TokenStream {
    let class_ident = format_ts_ident!(@class, "{}", struct_name);
    let (field_idents, field_tys): (Vec<_>, Vec<_>) = fields
        .into_iter()
        .map(|field| {
            (
                format_ts_ident!(@var, "{}", field.name),
                quote_ty(&field.ty),
            )
        })
        .unzip();
    quote! {
        export class #class_ident {
            #(public #field_idents: #field_tys;)*

            public constructor(fields: { #(#field_idents: #field_tys),* }) {
                #(this.#field_idents = fields.#field_idents;)*
            }

            #extra_members
        }
    }
}

fn generate_tuple_class<'a>(
    tuple_name: &str,
    tys: impl IntoIterator<Item = &'a oasis_rpc::Type> + std::iter::TrustedLen,
) -> TokenStream {
    let class_ident = format_ts_ident!(@class, "{}", tuple_name);
    let (field_idents, arg_idents): (Vec<_>, Vec<_>) = (0..tys.size_hint().0)
        .map(|i| {
            (
                proc_macro2::Literal::usize_unsuffixed(i),
                format_ident!("arg{}", i),
            )
        })
        .unzip();
    let field_tys: Vec<_> = tys.into_iter().map(|ty| quote_ty(ty)).collect();

    quote! {
        export class #class_ident {
            #(public #field_idents: #field_tys;)*

            public constructor(#(#arg_idents: #field_tys),*) {
                #(this[#field_idents] = #arg_idents;)*
            }
        }
    }
}

fn generate_rpc_functions<'a>(
    rpcs: &'a [oasis_rpc::Function],
) -> impl Iterator<Item = TokenStream> + 'a {
    rpcs.iter().map(move |rpc| {
        let (arg_idents, arg_tys): (Vec<_>, Vec<_>) = rpc
            .inputs
            .iter()
            .map(|inp| (format_ts_ident!(@var, "{}", inp.name), quote_ty(&inp.ty)))
            .unzip();

        let fn_ident = format_ts_ident!(@var, "{}", rpc.name);
        let ret_ty = if let Some(output_ty) = &rpc.output {
            quote_ty(output_ty)
        } else {
            quote!(void)
        };
        let err_handler = rpc.output.as_ref().and_then(|output| {
            if let oasis_rpc::Type::Result(_, err_ty) = &output {
                let neqeq = [
                    Punct::new('!', Spacing::Joint),
                    Punct::new('=', Spacing::Joint),
                    Punct::new('=', Spacing::Alone),
                ];
                let quot_err_ty = quote_ty(err_ty);
                Some(quote! {
                    if (typeof res.error #(#neqeq)* "undefined") {
                        throw abiDecode(#quot_err_ty, decodeHex(res.error));
                    }
                })
            } else {
                None
            }
        });
        let trailing_comma = if !rpc.inputs.is_empty() {
            quote!(,)
        } else {
            quote!()
        };
        quote! {
            public async #fn_ident(
                #(#arg_idents: #arg_tys),*#trailing_comma
                options?: RpcOptions
            ): Promise<#ret_ty> {
                const res = await this.gateway.rpc({
                    data: abiEncode(new Tuple([ #(#arg_idents),* ])),
                    address: this.address,
                    options,
                });
                #err_handler
                return abiDecode(#ret_ty, decodeHex(res.output));
            }
        }
    })
}

fn quote_ty(ty: &oasis_rpc::Type) -> TokenStream {
    use oasis_rpc::Type::*;
    match ty {
        Bool => quote!(boolean),
        U8 | I8 | U16 | I16 | U32 | I32 | U64 | I64 | F32 | F64 => quote!(number),
        Bytes => quote!(Uint8Array),
        String => quote!(string),
        Address => quote!(Address),
        Balance => quote!(Balance),
        Defined { namespace, ty } => {
            let ty_ident = format_ts_ident!(@class, "{}", ty);
            if let Some(ns) = namespace {
                let ns_ident = format_ts_ident!(@import, "{}", ns);
                quote!(#ns_ident.#ty_ident)
            } else {
                quote!(#ty_ident)
            }
        }
        Tuple(tys) => {
            let quot_tys = tys.iter().map(quote_ty);
            quote!([ #(#quot_tys),* ])
        }
        List(ty) | Array(ty, _) => {
            let quot_ty = quote_ty(ty);
            quote!(#quot_ty[])
        }
        Set(ty) => {
            let quot_ty = quote_ty(ty);
            quote!(Set<#quot_ty>)
        }
        Map(k_ty, v_ty) => {
            let quot_k_ty = quote_ty(k_ty);
            let quot_v_ty = quote_ty(v_ty);
            quote!(Map<#quot_k_ty, #quot_v_ty>)
        }
        Optional(ty) => {
            let quot_ty = quote_ty(ty);
            quote!(#quot_ty | undefined)
        }
        Result(ok_ty, _err_ty) => {
            let quot_ty = quote_ty(ok_ty);
            quote!(#quot_ty) // NOTE: ensure proper handling of error ty
        }
    }
}
