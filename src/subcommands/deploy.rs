pub fn deploy() -> Result<(), failure::Error> {
    match webbrowser::open("https://dashboard.oasiscloud.io/newcontract") {
        Ok(_) => Ok(()),
        Err(err) => Err(failure::format_err!(
            "failed to open browser for service deployment: {}",
            err
        )),
    }
}
