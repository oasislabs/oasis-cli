use std::path::Path;

use oasis_rpc::{
    import::{ImportLocation, ImportedService, Importer},
    Interface,
};

use crate::errors::Result;

pub fn ifextract(import_location: &str, out_dir: &std::path::Path) -> Result<()> {
    crate::emit!(cmd.ifextract);
    let import_location = if let Ok(url) = import_location.parse() {
        ImportLocation::Url(url)
    } else {
        ImportLocation::Path(std::path::PathBuf::from(import_location))
    };
    for ImportedService { interface, .. } in
        Importer::for_location(import_location, &std::env::current_dir().unwrap())?.import_all()?
    {
        if interface.name.contains(std::path::MAIN_SEPARATOR) {
            return Err(anyhow!("Malformed interface name: `{}`", interface.name));
        }
        let iface_pretty = interface.to_string().unwrap();
        if out_dir == std::path::Path::new("-") {
            println!("{}", iface_pretty);
        } else {
            std::fs::write(
                out_dir.join(format!("{}.json", interface.name)),
                iface_pretty.as_bytes(),
            )?;
        }
    }
    Ok(())
}

pub fn extract_interface(
    import_loc: ImportLocation,
    import_base_path: &Path,
) -> Result<Vec<Interface>> {
    Ok(Importer::for_location(import_loc, import_base_path)?
        .import_all()?
        .into_iter()
        .map(|imported_service| imported_service.interface)
        .collect())
}
