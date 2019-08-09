pub static SET_TOOLCHAIN: &str = r"VERSION FORMAT:
    VERSION must be a release string, `latest`, or `unstable`.

    A release string looks like `19.36` where the first number is the
    two-digit year and the second number is the week number. You can
    find a list of available versions at https://oasis.dev/releases.

    `latest` and `unstable` will resolve to the most recent release
    that is stable or non-broken, respectively.";
