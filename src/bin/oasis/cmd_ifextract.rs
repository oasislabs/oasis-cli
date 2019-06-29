pub fn ifextract(service_url: &str, out_dir: &std::path::Path) -> Result<(), failure::Error> {
    for iface in mantle_rpc::Importer::for_url(service_url)?.import_all()? {
        if iface.name.contains(std::path::MAIN_SEPARATOR) {
            return Err(failure::format_err!(
                "Malformed interface name: `{}`",
                iface.name
            ));
        }
        if out_dir == std::path::Path::new("-") {
            println!("{}", iface.to_string()?);
        } else {
            std::fs::write(
                out_dir.join(format!("{}.json", iface.name)),
                iface.to_string()?.as_bytes(),
            )?;
        }
    }
    Ok(())
}
