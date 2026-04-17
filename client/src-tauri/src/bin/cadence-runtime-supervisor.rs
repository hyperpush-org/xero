fn main() {
    if let Err(error) = cadence_desktop_lib::runtime::run_supervisor_sidecar_from_env() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
