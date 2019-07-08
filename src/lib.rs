pub mod build {
    use std::path::Path;

    pub fn prep_wasm(
        input_wasm: &Path,
        output_wasm: &Path,
        release: bool,
    ) -> Result<(), failure::Error> {
        let mut module = walrus::Module::from_file(input_wasm)?;

        externalize_mem(&mut module);

        if release {
            let customs_to_delete = module
                .customs
                .iter()
                .filter_map(|(id, custom)| {
                    if custom.name().starts_with("mantle") {
                        None
                    } else {
                        Some(id)
                    }
                })
                .collect::<Vec<_>>();
            for id in customs_to_delete {
                module.customs.delete(id);
            }
        }

        module.emit_wasm_file(output_wasm)?;

        Ok(())
    }

    fn externalize_mem(module: &mut walrus::Module) {
        let mem_export_id = module
            .exports
            .iter()
            .find(|e| e.name == "memory")
            .unwrap()
            .id();
        module.exports.delete(mem_export_id);

        let mut mem = module.memories.iter_mut().nth(0).unwrap();
        mem.import = Some(module.imports.add("env", "memory", mem.id()));
    }
}
