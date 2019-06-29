pub fn ifextract(service_url: &str, out_dir: &std::path::Path) -> Result<(), failure::Error> {
    for iface in mantle_rpc::Importer::for_url(service_url)?.import_all()? {
        std::fs::write(
            out_dir.join(format!("{}.json", iface.name)),
            iface.to_string()?.as_bytes(),
        )?;
    }
    Ok(())
}
