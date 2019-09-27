use oasis_rpc::import::{ImportLocation, ImportedService, Importer};

pub fn ifextract(import_location: &str, out_dir: &std::path::Path) -> Result<(), failure::Error> {
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
            return Err(failure::format_err!(
                "Malformed interface name: `{}`",
                interface.name
            ));
        }
        if out_dir == std::path::Path::new("-") {
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
