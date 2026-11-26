//! Source assertions for the update command surface. Native updater/shell
//! calls cannot run in a headless unit test, so these pin the production
//! ordering that keeps teardown safe: acquire every domain, shut the gate,
//! clean up, then install; validate URLs before handing them to the shell.

const UPDATE_SOURCE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/commands/update.rs");

fn update_source() -> String {
    std::fs::read_to_string(UPDATE_SOURCE_PATH).unwrap_or_default()
}

#[test]
fn install_source_cleans_up_before_download_and_install() {
    let source = update_source();
    let cleanup = source
        .find("cleanup(")
        .expect("install must call shared lifecycle cleanup");
    let install = source
        .find("update::install")
        .expect("install must call the update backend");
    assert!(
        cleanup < install,
        "proxy/core teardown must finish before the installer starts"
    );
}

#[test]
fn install_source_uses_shutdown_gate() {
    let source = update_source();
    let gate = source
        .find("begin_shutdown()")
        .expect("install must claim the idempotent shutdown gate");
    assert!(
        source[gate..].contains("UpdateBusy"),
        "a second terminal action must be rejected as update_busy"
    );
}

#[test]
fn open_source_validates_release_url_prefix() {
    let source = update_source();
    let validate = source
        .find("validate_release_url")
        .expect("open must validate the URL before launching it");
    let open = source
        .find("shell::open")
        .expect("open must use the shell opener");
    assert!(
        validate < open,
        "user-supplied URL must be checked before shell::open"
    );
}

/// Position of a guard's real `try_acquire()` call, not of its declaration or
/// any other textual mention. The name must be immediately followed (modulo
/// whitespace) by the acquire call.
fn try_acquire_position(source: &str, guard: &str) -> usize {
    source
        .match_indices(guard)
        .find_map(|(pos, _)| {
            let tail = &source[pos + guard.len()..];
            let call = tail.trim_start();
            call.starts_with(".try_acquire()")
                .then(|| pos + guard.len() + (tail.len() - call.len()))
        })
        .unwrap_or_else(|| panic!("{guard} must be acquired with try_acquire()"))
}

#[test]
fn install_source_serializes_domains_before_cleanup() {
    let source = update_source();
    let cleanup = source
        .find("cleanup(")
        .expect("install must call shared lifecycle cleanup");
    for guard in ["connection_guard", "probe_guard", "mutation_guard"] {
        let acquired = try_acquire_position(&source, guard);
        assert!(
            acquired < cleanup,
            "{guard}.try_acquire() must run before teardown starts"
        );
    }
}

#[test]
fn install_source_aborts_shutdown_gate_on_failure() {
    let source = update_source();
    let install = source
        .find("update::install")
        .expect("install must call the update backend");
    let abort = source
        .find("abort_shutdown()")
        .expect("failed install must release the shutdown gate");
    let error = source
        .find("Err(CommandError::Update)")
        .expect("failed install must map to the update error");
    assert!(
        install < abort && abort < error,
        "abort_shutdown() must run between the failed install and returning update error"
    );
}
