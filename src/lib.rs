pub mod build {
    use std::path::Path;

    pub fn prep_wasm(
        input_wasm: &Path,
        output_wasm: &Path,
        release: bool,
    ) -> Result<(), failure::Error> {
        let mut module = walrus::Module::from_file(input_wasm)?;

        remove_start_fn(&mut module);
        externalize_mem(&mut module);

        if release {
            module.customs = Default::default();
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

    fn remove_start_fn(module: &mut walrus::Module) {
        let mut start_fn_ids = None;
        for export in module.exports.iter() {
            if let walrus::ExportItem::Function(fn_id) = export.item {
                if export.name == "_start" {
                    start_fn_ids = Some((export.id(), fn_id));
                }
            }
        }
        if let Some((start_export_id, start_fn_id)) = start_fn_ids {
            module.exports.delete(start_export_id);
            module.funcs.delete(start_fn_id);
        }
    }
}
