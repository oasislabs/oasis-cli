use heck::*;
use oasis_rpc::Interface;
use proc_macro2::{Literal, Punct, Spacing, TokenStream};
use quote::{format_ident, quote};

macro_rules! format_ts_ident {
    (@module, $name:expr) => {
        format_ts_ident!(@raw, $name.to_kebab_case())
    };
    (@import, $name:expr) => {
        format_ts_ident!(@raw, $name.to_mixed_case())
    };
    (@class, $name:expr) => {
        format_ts_ident!(@raw, $name.to_camel_case())
    };
    (@var, $name:expr) => {
        format_ts_ident!(@raw, $name.to_mixed_case())
    };
    (@const, $name:expr) => {
        format_ts_ident!(@raw, $name.to_shouty_snake_case());
    };
    (@raw, $name:expr) => {
        format_ident!("{}", $name)
    };
}

pub fn generate(iface: &Interface) -> TokenStream {
    let service_ident = format_ts_ident!(@class, iface.name);

    let imports = iface.imports.iter().map(|imp| {
        let import_ident = format_ts_ident!(@class, imp.name);
        let module_name = format!("./{}", format_ts_ident!(@module, imp.name).to_string());
        quote!(import  #import_ident from #module_name)
    });

    let type_defs = generate_type_defs(&iface.type_defs);

    let rpc_functions = generate_rpc_functions(&iface.functions);

    quote! {
        import Gateway from "@oasislabs/gateway";
        import { RpcOptions } from "@oasislabs/service";
        import {
            OasisAbiEncodable,
            Address,
            Balance,
            Decoder as OasisAbiDecoder,
            Encoder as OasisAbiEncoder,
            Schema as OasisAbiSchema,
            abiDecode as oasisAbiDecode,
            abiEncode as oasisAbiEncode,
        } from "oasis-std";

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
                    let type_ident = format_ts_ident!(@class, name);

                    let is_tagged_union = variants.iter().any(|v| v.fields.is_some());
                    if !is_tagged_union {
                        let field_idents =
                            variants.iter().map(|v| format_ts_ident!(@class, v.name));
                        return quote! {
                            export enum #type_ident {
                                #(#field_idents),*
                            }
                        };
                    }

                    let variant_idents: Vec<_> = variants
                        .iter()
                        .map(|v| format_ts_ident!(@class, v.name))
                        .collect();
                    let variant_classes = variants.iter().map(|variant| match &variant.fields {
                        Some(oasis_rpc::EnumFields::Named(fields)) => {
                            let is_tuple = fields
                                .iter()
                                .enumerate()
                                .all(|(i, field)| field.name == i.to_string());
                            if !is_tuple {
                                generate_struct_class(&variant.name, fields, quote!())
                            } else {
                                generate_tuple_class(
                                    &variant.name,
                                    &fields.iter().map(|f| f.ty.clone()).collect::<Vec<_>>(),
                                )
                            }
                        }
                        Some(oasis_rpc::EnumFields::Tuple(tys)) => {
                            generate_tuple_class(&variant.name, &tys)
                        }
                        None => generate_tuple_class(&variant.name, &[] /* fields */),
                    });

                    quote! {
                        module #type_ident {
                            #(#variant_classes)*

                            export function abiDecode(decoder: OasisAbiDecoder): #type_ident {
                                const variantId = decoder.readU32();
                                return (#type_ident as any).VARIANTS[variantId].oasisAbiDecode(decoder);
                            }

                            const VARIANTS: Function[] = [ #(#variant_idents),* ];
                        }
                        export type #type_ident = #(#type_ident.#variant_idents)|*;
                    }
                }
                TypeDef::Event {
                    name,
                    fields: indexed_fields,
                } => {
                    let topic_names: Vec<_> = indexed_fields
                        .iter()
                        .map(|f| format_ts_ident!(@var, &f.name))
                        .collect();
                    let topic_tys = indexed_fields.iter().map(|f| quote_ty(&f.ty));
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
                    generate_struct_class(name, &fields, extra_members)
                }
            }
        })
        .collect()
}

fn generate_struct_class<'a>(
    struct_name: &str,
    fields: &'a [oasis_rpc::Field],
    extra_members: TokenStream,
) -> TokenStream {
    let class_ident = format_ts_ident!(@class, struct_name);

    let field_idents: Vec<_> = fields
        .iter()
        .map(|field| format_ts_ident!(@var, field.name))
        .collect();
    let field_tys: Vec<_> = fields.iter().map(|field| quote_ty(&field.ty)).collect();
    let field_schema_tys: Vec<_> = fields
        .iter()
        .map(|field| quote_schema_ty(&field.ty))
        .collect();

    quote! {
        export class #class_ident implements OasisAbiEncodable {
            #(public #field_idents: #field_tys;)*

            public constructor(fields: { #(#field_idents: #field_tys),* }) {
                #(this.#field_idents = fields.#field_idents;)*
            }

            public abiEncode(encoder: OasisAbiEncoder) {
                #(oasisAbiEncode(#field_schema_tys as OasisAbiSchema, this.#field_idents, encoder);)*
            }

            public static abiDecode(decoder: OasisAbiDecoder): #class_ident {
                return new #class_ident({
                    #(#field_idents: oasisAbiDecode(#field_schema_tys as OasisAbiSchema, decoder)),*
                });
            }

            #extra_members
        }
    }
}

fn generate_tuple_class(tuple_name: &str, tys: &[oasis_rpc::Type]) -> TokenStream {
    let class_ident = format_ts_ident!(@class, tuple_name);
    let (field_idents, arg_idents): (Vec<_>, Vec<_>) = (0..tys.len())
        .map(|i| {
            (
                proc_macro2::Literal::usize_unsuffixed(i),
                format_ident!("arg{}", i),
            )
        })
        .unzip();
    let field_tys: Vec<_> = tys.iter().map(|ty| quote_ty(ty)).collect();
    let field_schema_tys: Vec<_> = tys.iter().map(quote_schema_ty).collect();

    quote! {
        export class #class_ident implements OasisAbiEncodable {
            #(public #field_idents: #field_tys;)*

            public constructor(#(#arg_idents: #field_tys),*) {
                #(this[#field_idents] = #arg_idents;)*
            }

            public abiEncode(encoder: OasisAbiEncoder) {
                #(oasisAbiEncode(#field_schema_tys as OasisAbiSchema, this[#field_idents], encoder));*
            }

            public static abiDecode(decoder: OasisAbiDecoder): #class_ident {
                return new #class_ident(
                    #(oasisAbiDecode(#field_schema_tys as OasisAbiSchema, decoder)),*
                );
            }
        }
    }
}

fn generate_rpc_functions<'a>(
    rpcs: &'a [oasis_rpc::Function],
) -> impl Iterator<Item = TokenStream> + 'a {
    rpcs.iter().map(move |rpc| {
        let arg_idents: Vec<_> = rpc
            .inputs
            .iter()
            .map(|inp| format_ts_ident!(@var, inp.name))
            .collect();
        let arg_tys = rpc.inputs.iter().map(|inp| quote_ty(&inp.ty));
        let arg_schema_tys = rpc.inputs.iter().map(|inp| quote_schema_ty(&inp.ty));

        let fn_ident = format_ts_ident!(@var, rpc.name);
        let rpc_ret_ty = rpc
            .output
            .as_ref()
            .map(|o| quote_ty(o))
            .unwrap_or_else(|| quote!(void));
        let returner = rpc
            .output
            .as_ref()
            .and_then(|output| {
                use oasis_rpc::Type::{Result, Tuple};
                match output {
                    Tuple(tys) | Result(box Tuple(tys), _) if tys.is_empty() => None,
                    _ => {
                        let quot_schema_ty = quote_schema_ty(output);
                        Some(quote!(return oasisAbiDecode(#quot_schema_ty as OasisAbiSchema, res.output);))
                    }
                }
            })
            .unwrap_or_else(|| quote!(return;));
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
                        throw oasisAbiDecode(#quot_err_ty as OasisAbiSchema, res.error);
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
            ): Promise<#rpc_ret_ty> {
                const res = await this.gateway.rpc({
                    data: oasisAbiEncode([#(#arg_schema_tys as OasisAbiSchema),*], [ #(#arg_idents),* ]),
                    address: this.address.bytes,
                    options,
                });
                #err_handler
                #returner
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
            let ty_ident = format_ts_ident!(@class, ty);
            if let Some(ns) = namespace {
                let ns_ident = format_ts_ident!(@import, ns);
                quote!(#ns_ident.#ty_ident)
            } else {
                quote!(#ty_ident)
            }
        }
        Tuple(tys) => {
            if tys.is_empty() {
                quote!(void)
            } else {
                let quot_tys = tys.iter().map(quote_ty);
                quote!([ #(#quot_tys),* ])
            }
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

fn quote_schema_ty(ty: &oasis_rpc::Type) -> TokenStream {
    use oasis_rpc::Type::*;
    match ty {
        Bool => quote!("boolean"),
        U8 => quote!("u8"),
        I8 => quote!("i8"),
        U16 => quote!("u16"),
        I16 => quote!("i16"),
        U32 => quote!("u32"),
        I32 => quote!("i32"),
        U64 => quote!("u64"),
        I64 => quote!("i64"),
        F32 => quote!("f32"),
        F64 => quote!("f64"),
        Bytes => quote!(["u8"]),
        String => quote!("string"),
        Address => quote!(Address),
        Balance => quote!(Balance),
        Defined { namespace, ty } => {
            let ty_ident = format_ts_ident!(@class, ty);
            if let Some(ns) = namespace {
                let ns_ident = format_ts_ident!(@import, ns);
                quote!(#ns_ident.#ty_ident)
            } else {
                quote!(#ty_ident)
            }
        }
        Tuple(tys) => {
            let quot_tys = tys.iter().map(quote_schema_ty);
            quote!([ #(#quot_tys),* ])
        }
        Array(ty, len) => {
            let quot_ty = quote_schema_ty(ty);
            let quot_len = Literal::u64_unsuffixed(*len);
            quote!([ #quot_ty, #quot_len ])
        }
        List(ty) => {
            let quot_ty = quote_schema_ty(ty);
            quote!([#quot_ty, Number.POSITIVE_INFINITY])
        }
        Set(ty) => {
            let quot_ty = quote_schema_ty(ty);
            quote!(["Set", #quot_ty])
        }
        Map(k_ty, v_ty) => {
            let quot_k_ty = quote_schema_ty(k_ty);
            let quot_v_ty = quote_schema_ty(v_ty);
            quote!(["Map", <#quot_k_ty, #quot_v_ty>])
        }
        Optional(ty) => {
            let quot_ty = quote_schema_ty(ty);
            quote!(["Option", #quot_ty])
        }
        Result(ok_ty, _err_ty) => {
            let quot_ty = quote_schema_ty(ok_ty);
            quote!(#quot_ty) // NOTE: ensure proper (downstream) handling of error ty
        }
    }
}
