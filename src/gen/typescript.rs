use heck::*;
use oasis_rpc::Interface;
use proc_macro2::{Ident, TokenStream};
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

struct Schema {
    pub ident: Ident,
}

impl Schema {
    fn new(iface: &Interface) -> Self {
        Self {
            ident: format_ts_ident!(@const, "{}_SCHEMA", iface.name),
        }
    }
}

pub struct TypescriptClient {
    module_ident: Ident,
}

pub fn generate(iface: &Interface) -> TokenStream {
    let service_ident = format_ts_ident!(@class, "{}", iface.name);

    let imports = iface.imports.iter().map(|imp| {
        let import_ident = format_ts_ident!(@class, "{}", imp.name);
        let module_ident = format_ts_ident!(@module, "{}", imp.name);
        quote!(import * as #import_ident from concat!("./", stringify!(#module_ident)))
    });

    let mut schema = Schema::new(iface);

    let type_defs = generate_type_defs(&iface.type_defs, &mut schema);

    let rpc_functions = generate_rpc_functions(&iface.functions, &mut schema);

    quote! {
        import { Address, Balance, Gateway, RpcOptions } from "@oasislabs/service";

        enum Result {
            Ok,
            Err,
        }

        #(#imports)*

        #(#type_defs)*

        export class #service_ident {
            public constructor(public address: Address, private gateway: Gateway) {}

            #(#rpc_functions)*
        }

        export default #service_ident;
    }
}

fn generate_type_defs(type_defs: &[oasis_rpc::TypeDef], schema: &mut Schema) -> Vec<TokenStream> {
    type_defs
        .iter()
        .map(|type_def| {
            use oasis_rpc::TypeDef;

            // schema.register_type(type_ident, type_schema); // TODO

            match type_def {
                TypeDef::Struct { name, fields } => generate_struct_class(name, fields),
                TypeDef::Enum { name, variants } => {
                    let type_ident = format_ts_ident!(@class, "{}", name);
                    let is_tagged_union = variants.iter().any(|v| v.fields.is_some());
                    if !is_tagged_union {
                        let field_names = variants
                            .iter()
                            .map(|v| format_ts_ident!(@class, "{}", v.name));
                        quote! {
                            export enum #type_ident {
                                #(#field_names),*
                            }
                        }
                    } else {
                        let variant_idents: Vec<_> = variants.iter().map(|v| &v.name).collect();
                        let variant_classes = variants.iter().filter_map(|variant| {
                            let fields = match &variant.fields {
                                Some(fields) => fields,
                                None => return None,
                            };

                            Some(match fields {
                                oasis_rpc::EnumFields::Named(fields) => {
                                    let is_tuple = fields
                                        .iter()
                                        .enumerate()
                                        .all(|(i, field)| field.name == i.to_string());
                                    if !is_tuple {
                                        generate_struct_class(&variant.name, fields.iter())
                                    } else {
                                        generate_tuple_class(
                                            &variant.name,
                                            fields.iter().map(|f| &f.ty),
                                        )
                                    }
                                }
                                oasis_rpc::EnumFields::Tuple(tys) => {
                                    generate_tuple_class(&variant.name, tys.iter())
                                }
                            })
                        });
                        quote! {
                            module #type_ident {
                                const VARIANTS: string[] = [ #(stringify!(#variant_idents)),* ];

                                #(
                                    export class #variant_idents {
                                        public static kind = stringify!(#variant_idents);
                                        #variant_classes
                                    }
                                )*
                            }
                        }
                    }
                }
                TypeDef::Event { name, fields } => unimplemented!(),
            }
        })
        .collect()
}

fn generate_struct_class<'a>(
    struct_name: &str,
    fields: impl IntoIterator<Item = &'a oasis_rpc::Field>,
) -> TokenStream {
    let class_ident = format_ts_ident!(@class, "{}", struct_name);
    let (field_idents, field_user_tys): (Vec<_>, Vec<_>) = fields
        .into_iter()
        .map(|field| {
            (
                format_ts_ident!(@var, "{}", field.name),
                quote_user_ty(&field.ty),
            )
        })
        .unzip();
    quote! {
        export class #class_ident {
            #(public #field_idents: #field_user_tys;)*

            public constructor(fields: { #(#field_idents: #field_user_tys),* }) {
                #(this.#field_idents = fields.#field_idents;)*
            }
        }
    }
}

fn generate_tuple_class<'a>(
    tuple_name: &str,
    tys: impl IntoIterator<Item = &'a oasis_rpc::Type> + std::iter::TrustedLen,
) -> TokenStream {
    let class_ident = format_ts_ident!(@class, "{}", tuple_name);
    let (field_idents, arg_idents): (Vec<_>, Vec<_>) = (0..tys.size_hint().0)
        .map(|i| (format_ident!("{}", i), format_ident!("arg{}", i)))
        .unzip();
    let field_user_tys: Vec<_> = tys.into_iter().map(|ty| quote_user_ty(ty)).collect();

    quote! {
        export class #class_ident {
            #(public #field_idents: #field_user_tys;)*

            public constructor(#(#arg_idents: #field_user_tys),*) {
                #(this.#field_idents = #arg_idents;)*
            }
        }
    }
}

fn generate_rpc_functions<'a>(
    rpcs: &'a [oasis_rpc::Function],
    schema: &'a mut Schema,
) -> impl Iterator<Item = TokenStream> + 'a {
    let schema_ident = &schema.ident;
    rpcs.iter().map(move |rpc| {
        let (arg_idents, user_arg_tys): (Vec<_>, Vec<_>) = rpc
            .inputs
            .iter()
            .map(|inp| {
                (
                    format_ts_ident!(@var, "{}", inp.name),
                    quote_user_ty(&inp.ty),
                )
            })
            .unzip();

        let fn_ident = format_ts_ident!(@var, "{}", rpc.name);
        let (user_ret_ty, schema_ret_ty) = if let Some(output_ty) = &rpc.output {
            (quote_user_ty(&output_ty), quote_schema_ty(&output_ty))
        } else {
            (quote!(void), quote!(null)) // TODO: add null read/write support to borsh
        };
        quote! {
            public async #fn_ident(
                #(#arg_idents: #user_arg_tys),*,
                options?: RpcOptions
            ): Promise<#user_ret_ty> {
                const payload = borsh.serializeTuple(#schema_ident, [ #(#arg_idents),* ]);
                const res = await this.gateway.rpc({
                    data: payload,
                    address: this.address,
                    options,
                });
                if (typeof res.error !== "undefined") {
                    throw new ExecutionError(borsh.deserialize(#schema_ident, #schema_ret_ty, res.error))
                }
                return borsh.deserialize(#schema_ident, #schema_ret_ty, res.output);
            }
        }
    })
}

fn quote_user_ty(ty: &oasis_rpc::Type) -> TokenStream {
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
            let quot_tys = tys.iter().map(quote_user_ty);
            quote!([ #(#quot_tys),* ])
        }
        List(ty) | Array(ty, _) => {
            let quot_ty = quote_user_ty(ty);
            quote!(#quot_ty[])
        }
        Set(ty) => {
            let quot_ty = quote_user_ty(ty);
            quote!(Set<#quot_ty>)
        }
        Map(k_ty, v_ty) => {
            let quot_k_ty = quote_user_ty(k_ty);
            let quot_v_ty = quote_user_ty(v_ty);
            quote!(Map<#quot_k_ty, #quot_v_ty>)
        }
        Optional(ty) => {
            let quot_ty = quote_user_ty(ty);
            quote!(#quot_ty | undefined)
        }
        Result(_ok_ty, _err_ty) => {
            let quot_ty = quote_user_ty(ty);
            quote!(#quot_ty) // this is implemented as a rejected promise
        }
    }
}

fn quote_schema_ty(ty: &oasis_rpc::Type) -> TokenStream {
    use oasis_rpc::Type::*;
    match ty {
        // TODO: add Boolean and F(32|64) support to borsh-ts
        Bool => quote!("Boolean"),
        U8 | I8 => quote!("U8"),
        U16 | I16 => quote!("U16"),
        U32 | I32 => quote!("U32"),
        U64 | I64 => quote!("U64"),
        F32 => quote!("F32"),
        F64 => quote!("F64"),
        Bytes => quote!(["U8"]),
        String => quote!("String"),
        Address => quote!(["U8", 20]),
        Balance => quote!([16]),
        Defined { namespace, ty } => {
            let ty_ident = format_ts_ident!(@class, "{}", ty);
            let def_ty_ident = if let Some(ns) = namespace {
                let ns_ident = format_ts_ident!(@import, "{}", ns);
                format_ident!("{}_{}", ns_ident, ty_ident)
            } else {
                ty_ident
            };
            quote!(#def_ty_ident)
        }
        Tuple(tys) => {
            let quot_tys = tys.iter().map(quote_user_ty);
            quote!([ #(#quot_tys),* ])
        }
        Array(ty, len) => {
            let quot_ty = quote_schema_ty(ty);
            quote!([ #len, #quot_ty ])
        }
        List(ty) => {
            let quot_ty = quote_schema_ty(ty);
            quote!([#quot_ty])
        }
        Set(ty) => {
            let quot_ty = quote_schema_ty(ty);
            quote!(["Set", #quot_ty])
        }
        Map(k_ty, v_ty) => {
            let quot_k_ty = quote_schema_ty(k_ty);
            let quot_v_ty = quote_schema_ty(v_ty);
            quote!(Map<#quot_k_ty, #quot_v_ty>)
        }
        Optional(ty) => {
            let quot_ty = quote_schema_ty(ty);
            quote!(#quot_ty | undefined)
        }
        Result(_ok_ty, _err_ty) => {
            let quot_ty = quote_schema_ty(ty);
            quote!(#quot_ty) // this is implemented as a rejected promise
        }
    }
}
