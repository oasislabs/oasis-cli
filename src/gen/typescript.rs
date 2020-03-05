use heck::*;
use oasis_rpc::Interface;
use proc_macro2::{Ident, Literal, TokenStream};
use quote::{format_ident, quote};

macro_rules! format_ts_ident {
    (@import, $name:expr) => {
        format_ts_ident!(@raw, $name.to_mixed_case())
    };
    (@class, $name:expr) => {
        format_ts_ident!(@raw, $name.to_camel_case())
    };
    (@var, $name:expr) => {
        format_ts_ident!(@raw, var_name(&$name))
    };
    (@const, $name:expr) => {
        format_ts_ident!(@raw, $name.to_shouty_snake_case());
    };
    (@raw, $name:expr) => {
        format_ident!("{}", $name)
    };
}

pub fn generate(iface: &Interface, bytecode_url: &url::Url) -> TokenStream {
    let service_ident = format_ts_ident!(@class, iface.name);
    let bytecode_url_str = bytecode_url.as_str();

    let imports = iface.imports.iter().map(|imp| {
        let import_ident = format_ts_ident!(@var, imp.name);
        let import_path = format!("./{}", module_name(&imp.name));
        quote!(import * as #import_ident from #import_path;)
    });

    let type_defs = generate_type_defs(&iface.type_defs);

    let deploy_function = generate_deploy_function(&service_ident, &iface.constructor);
    let rpc_functions = generate_rpc_functions(&service_ident, &iface.functions);

    quote! {
        import { Buffer } from "buffer";
        import * as oasis from "oasis-std";

        #(#imports)*

        #(#type_defs)*

        export class #service_ident {
            public static BYTECODE_URL = #bytecode_url_str;

            private constructor(readonly address: oasis.Address, private gateway: oasis.Gateway) {}

            public static async connect(
                address: oasis.Address,
                gateway: oasis.Gateway
            ): Promise<#service_ident> {
                return new #service_ident(address, gateway);
            }

            #deploy_function

            #(#rpc_functions)*
        }
    }
}

fn generate_type_defs(type_defs: &[oasis_rpc::TypeDef]) -> Vec<TokenStream> {
    type_defs
        .iter()
        .map(|type_def| {
            use oasis_rpc::TypeDef;

            match type_def {
                TypeDef::Struct { name, fields } => {
                    generate_struct_class(name, fields, quote!(), None)
                }
                TypeDef::Enum { name, variants } => {
                    let type_ident = format_ts_ident!(@class, name);

                    let variant_idents: Vec<_> = variants
                        .iter()
                        .map(|v| format_ts_ident!(@class, v.name))
                        .collect();
                    let variant_classes =
                        variants
                            .iter()
                            .enumerate()
                            .map(|(i, variant)| match &variant.fields {
                                Some(oasis_rpc::EnumFields::Named(fields)) => {
                                    let is_tuple = fields
                                        .iter()
                                        .enumerate()
                                        .all(|(i, field)| field.name == i.to_string());
                                    if !is_tuple {
                                        generate_struct_class(
                                            &variant.name,
                                            fields,
                                            quote!(),
                                            Some(i),
                                        )
                                    } else {
                                        generate_tuple_class(
                                            &variant.name,
                                            &fields
                                                .iter()
                                                .map(|f| f.ty.clone())
                                                .collect::<Vec<_>>(),
                                            quote!(),
                                            Some(i),
                                        )
                                    }
                                }
                                Some(oasis_rpc::EnumFields::Tuple(tys)) => {
                                    generate_tuple_class(&variant.name, &tys, quote!(), Some(i))
                                }
                                None => generate_tuple_class(
                                    &variant.name,
                                    &[], /* no fields */
                                    quote!(),
                                    Some(i),
                                ),
                            });

                    quote! {
                        export module #type_ident {
                            #(#variant_classes)*

                            export function abiDecode(decoder: oasis.Decoder): #type_ident {
                                const variantId = decoder.readU8();
                                return (#type_ident as any).VARIANTS[variantId].abiDecode(decoder);
                            }

                            export const VARIANTS: Function[] = [ #(#variant_idents),* ];
                        }
                        export type #type_ident = #(#type_ident.#variant_idents)|*;
                    }
                }
                TypeDef::Event {
                    name,
                    fields: indexed_fields,
                } => {
                    let event_ident = format_ts_ident!(@class, name);
                    let topic_names = indexed_fields.iter().map(|f| var_name(&f.name));
                    let topic_idents: Vec<_> = indexed_fields
                        .iter()
                        .map(|f| format_ts_ident!(@var, &f.name))
                        .collect();
                    let topic_tys = indexed_fields.iter().map(|f| quote_ty(&f.ty));
                    let topic_schema_tys = indexed_fields.iter().map(|f| quote_schema_ty(&f.ty));
                    let topics_arg = if !indexed_fields.is_empty() {
                        quote!(topics?: { #(#topic_idents?: #topic_tys),* })
                    } else {
                        quote!()
                    };
                    let maybe_dot = make_operator("?.");

                    let extra_members = quote! {
                        public static async subscribe(
                            gateway: oasis.Gateway,
                            address: oasis.Address | null,
                            #topics_arg
                        ): Promise<oasis.Subscription<#event_ident>> {
                            const encodedTopics = [
                                oasis.encodeEventTopic("string", #event_ident.name),
                            ];
                            #(
                                if (topics #maybe_dot hasOwnProperty(#topic_names)) {
                                    encodedTopics.push(
                                        oasis.encodeEventTopic(
                                            #topic_schema_tys,
                                            topics.#topic_idents,
                                        )
                                    );
                                }
                            )*
                            return gateway.subscribe(
                                address,
                                encodedTopics,
                                async (payload: Uint8Array) => {
                                    return oasis.abiDecode(#event_ident, payload);
                                }
                            );
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
                    generate_struct_class(name, &fields, extra_members, None)
                }
            }
        })
        .collect()
}

fn generate_struct_class<'a>(
    struct_name: &str,
    fields: &'a [oasis_rpc::Field],
    extra_members: TokenStream,
    variant_idx: Option<usize>,
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

    let variant_encoder = variant_idx.map(|idx| {
        let idx_lit = Literal::usize_unsuffixed(idx);
        quote!(encoder.writeU8(#idx_lit);)
    });

    quote! {
        export class #class_ident implements oasis.AbiEncodable {
            #(public #field_idents: #field_tys;)*

            public constructor(fields: { #(#field_idents: #field_tys),* }) {
                #(this.#field_idents = fields.#field_idents;)*
            }

            public abiEncode(encoder: oasis.Encoder) {
                #variant_encoder
                #(oasis.abiEncode(#field_schema_tys as oasis.Schema, this.#field_idents, encoder);)*
            }

            public static abiDecode(decoder: oasis.Decoder): #class_ident {
                return new #class_ident({
                    #(#field_idents: oasis.abiDecode(#field_schema_tys as oasis.Schema, decoder)),*
                });
            }

            #extra_members
        }
    }
}

fn generate_tuple_class(
    tuple_name: &str,
    tys: &[oasis_rpc::Type],
    extra_members: TokenStream,
    variant_idx: Option<usize>,
) -> TokenStream {
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

    let variant_encoder = variant_idx.map(|idx| {
        let idx_lit = Literal::usize_unsuffixed(idx);
        quote!(encoder.writeU8(#idx_lit);)
    });

    quote! {
        export class #class_ident implements oasis.AbiEncodable {
            #(public #field_idents: #field_tys;)*

            public constructor(#(#arg_idents: #field_tys),*) {
                #(this[#field_idents] = #arg_idents;)*
            }

            public abiEncode(encoder: oasis.Encoder) {
                #variant_encoder
                #(oasis.abiEncode(#field_schema_tys as oasis.Schema, this[#field_idents], encoder));*
            }

            public static abiDecode(decoder: oasis.Decoder): #class_ident {
                return new #class_ident(
                    #(oasis.abiDecode(#field_schema_tys as oasis.Schema, decoder)),*
                );
            }

            #extra_members
        }
    }
}

fn generate_deploy_function(service_ident: &Ident, ctor: &oasis_rpc::Constructor) -> TokenStream {
    let arg_idents: Vec<_> = ctor
        .inputs
        .iter()
        .map(|field| format_ts_ident!(@var, field.name))
        .collect();
    let arg_tys: Vec<_> = ctor
        .inputs
        .iter()
        .map(|field| quote_ty(&field.ty))
        .collect();
    let arg_schema_tys = ctor.inputs.iter().map(|field| quote_schema_ty(&field.ty));

    let deploy_try_catch = gen_rpc_err_handler(
        ctor.error.as_ref(),
        quote! {
            const deployedAddr = await gateway.deploy(
                await #service_ident.makeDeployPayload(#(#arg_idents),*),
                options,
            );
            return new #service_ident(deployedAddr, gateway);
        },
    );

    quote! {
        public static async deploy(
            gateway: oasis.Gateway,
            { #(#arg_idents),* }: { #(#arg_idents: #arg_tys);* },
            options?: oasis.DeployOptions,
        ): Promise<#service_ident> {
            #deploy_try_catch
        }

        public static async makeDeployPayload(#(#arg_idents: #arg_tys,)*): Promise<Buffer> {
            const encoder = new oasis.Encoder();
            encoder.writeU8Array(await oasis.fetchBytecode(#service_ident.BYTECODE_URL));
            return oasis.abiEncode(
                [ #(#arg_schema_tys as oasis.Schema),* ],
                [ #(#arg_idents),* ],
                encoder
            );
        }
    }
}

fn generate_rpc_functions<'a>(
    service_ident: &'a Ident,
    rpcs: &'a [oasis_rpc::Function],
) -> impl Iterator<Item = TokenStream> + 'a {
    rpcs.iter().enumerate().map(move |(i, rpc)| {
        let fn_id_lit = Literal::usize_unsuffixed(i);

        let arg_idents: Vec<_> = rpc
            .inputs
            .iter()
            .map(|inp| format_ts_ident!(@var, inp.name))
            .collect();
        let arg_tys: Vec<_> = rpc.inputs.iter().map(|inp| quote_ty(&inp.ty)).collect();
        let arg_schema_tys = rpc.inputs.iter().map(|inp| quote_schema_ty(&inp.ty));

        let fn_ident = format_ts_ident!(@var, rpc.name);
        let make_payload_ident = format_ident!("make{}Payload", rpc.name.to_camel_case());
        let rpc_ret_ty = rpc
            .output
            .as_ref()
            .map(|out_ty| {
                quote_ty(match out_ty {
                    oasis_rpc::Type::Result(box ok_ty, _) => ok_ty,
                    _ => out_ty,
                })
            })
            .unwrap_or_else(|| quote!(void));
        let returner = rpc
            .output
            .as_ref()
            .and_then(|output| {
                use oasis_rpc::Type::{Result, Tuple};
                match output {
                    Tuple(tys) | Result(box Tuple(tys), _) if tys.is_empty() => None,
                    oasis_rpc::Type::Result(box ok_ty, _) => {
                        let quot_schema_ty = quote_schema_ty(ok_ty);
                        //^ unwrap one layer of result, as the outer error is derived
                        // from the tx status code.
                        Some(quote! {
                            return oasis.abiDecode(#quot_schema_ty as oasis.Schema, res);
                        })
                    }
                    _ => {
                        let quot_schema_ty = quote_schema_ty(output);
                        Some(quote! {
                            return oasis.abiDecode(#quot_schema_ty as oasis.Schema, res);
                        })
                    }
                }
            })
            .unwrap_or_else(|| quote!(return;));
        let rpc_try_catch = gen_rpc_err_handler(
            rpc.output.as_ref().and_then(|output| {
                if let oasis_rpc::Type::Result(_, box err_ty) = output {
                    Some(err_ty)
                } else {
                    None
                }
            }),
            quote! {
                const res = await this.gateway.rpc(this.address, payload, options);
                #returner
            },
        );

        quote! {
            public async #fn_ident(
                #(#arg_idents: #arg_tys,)*
                options?: oasis.RpcOptions
            ): Promise<#rpc_ret_ty> {
                const payload = #service_ident.#make_payload_ident(#(#arg_idents),*);
                #rpc_try_catch
            }

            public static #make_payload_ident(#(#arg_idents: #arg_tys,)*): Buffer {
                const encoder = new oasis.Encoder();
                encoder.writeU8(#fn_id_lit);
                return oasis.abiEncode(
                    [ #(#arg_schema_tys as oasis.Schema),* ],
                    [ #(#arg_idents),* ],
                    encoder
                );
            }
        }
    })
}

fn quote_ty(ty: &oasis_rpc::Type) -> TokenStream {
    use oasis_rpc::Type::*;
    match ty {
        Bool => quote!(boolean),
        U8 | I8 | U16 | I16 | U32 | I32 | F32 | F64 => quote!(number),
        U64 | I64 => quote!(BigInt),
        Bytes => quote!(Uint8Array),
        String => quote!(string),
        Address => quote!(oasis.Address),
        Balance => quote!(oasis.Balance),
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
        List(box U8) | Array(box U8, _) => quote!(Uint8Array),
        List(box I8) | Array(box I8, _) => quote!(Int8Array),
        List(box U16) | Array(box U16, _) => quote!(Uint16Array),
        List(box I16) | Array(box I16, _) => quote!(Int16Array),
        List(box U32) | Array(box U32, _) => quote!(Uint32Array),
        List(box I32) | Array(box I32, _) => quote!(Int32Array),
        List(box U64) | Array(box U64, _) => quote!(BigUint64Array),
        List(box I64) | Array(box I64, _) => quote!(BigInt64Array),
        List(box F32) | Array(box F32, _) => quote!(Float32Array),
        List(box F64) | Array(box F64, _) => quote!(Float64Array),
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
        Result(ok_ty, err_ty) => {
            let quot_ok_ty = quote_ty(ok_ty);
            let quot_err_ty = quote_ty(err_ty);
            quote!(oasis.Result<#quot_ok_ty, #quot_err_ty>)
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
        Bytes => quote!(["u8", Number.POSITIVE_INFINITY]),
        String => quote!("string"),
        Address => quote!(oasis.Address),
        Balance => quote!(oasis.Balance),
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
        Result(ok_ty, err_ty) => {
            let quot_ok_ty = quote_schema_ty(ok_ty);
            let quot_err_ty = quote_schema_ty(err_ty);
            quote!(["Result", #quot_ok_ty, #quot_err_ty])
        }
    }
}

fn gen_rpc_err_handler(err_ty: Option<&oasis_rpc::Type>, try_block: TokenStream) -> TokenStream {
    let err_handler = err_ty.map(|err_ty| {
        let quot_schema_err_ty = quote_ty(err_ty);
        let maybe_dot = make_operator("?.");
        let eq3 = make_operator("===");
        quote! {
            if (e instanceof oasis.ExecutionError ||
                e.constructor #maybe_dot name #eq3 "ExecutionError") {
                throw oasis.abiDecode(#quot_schema_err_ty as oasis.Schema, e.output);
            }
        }
    });
    quote! {
        try {
            #try_block
        } catch (e) {
            #err_handler
            throw e;
        }
    }
}

pub fn module_name(iface_name: impl AsRef<str>) -> String {
    iface_name.as_ref().to_kebab_case()
}

fn var_name(name: &str) -> String {
    name.to_mixed_case()
}

pub fn make_operator(chars: &str) -> TokenStream {
    use proc_macro2::{Punct, Spacing, TokenTree};
    chars
        .chars()
        .enumerate()
        .map(|(i, ch)| -> TokenTree {
            Punct::new(
                ch,
                if i == chars.len() - 1 {
                    Spacing::Alone
                } else {
                    Spacing::Joint
                },
            )
            .into()
        })
        .collect()
}
