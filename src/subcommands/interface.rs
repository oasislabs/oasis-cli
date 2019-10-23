use std::path::{Path, PathBuf};

use oasis_rpc::import::{ImportLocation, ImportedService, Importer};

pub fn ifextract(import_location: &str, out_dir: &Path) -> failure::Fallible<()> {
    crate::emit!(cmd.ifextract);
    let import_location = if let Ok(url) = import_location.parse() {
        ImportLocation::Url(url)
    } else {
        ImportLocation::Path(PathBuf::from(import_location))
    };
    for ImportedService { interface, .. } in
        Importer::for_location(import_location, &std::env::current_dir().unwrap())?.import_all()?
    {
        if interface.name.contains(std::path::MAIN_SEPARATOR) {
            return Err(failure::format_err!(
                "Malformed interface name: `{}`",
                interface.name
            ));
        }
        if out_dir == Path::new("-") {
            println!("{}", interface.to_string()?);
        } else {
            std::fs::write(
                out_dir.join(format!("{}.json", interface.name)),
                interface.to_string()?.as_bytes(),
            )?;
        }
    }
    Ok(())
}

pub fn ifattach(service_wasm: &Path, iface_json: &Path) -> failure::Fallible<()> {
    crate::emit!(cmd.ifattach);
    let iface: oasis_rpc::Interface = serde_json::from_slice(&std::fs::read(iface_json)?)?;
    let mut module = walrus::Module::from_file(&service_wasm)?;
    module.customs.add(walrus::RawCustomSection {
        name: "oasis-interface".to_string(),
        data: iface.to_vec()?,
    });
    module.emit_wasm_file(service_wasm)?;
    Ok(())
}
