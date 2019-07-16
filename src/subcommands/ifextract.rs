use oasis_rpc::import::{ImportedService, Importer};

pub fn ifextract(service_url: &str, out_dir: &std::path::Path) -> Result<(), failure::Error> {
    crate::emit!(cmd.ifextract);
    for ImportedService { interface, .. } in
        Importer::for_url(service_url, std::env::current_dir().unwrap())?.import_all()?
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
