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

#[test]
fn install_source_waits_for_all_domain_permits_with_one_shot_removed() {
    let source = update_source();
    let cleanup = source
        .find("cleanup(")
        .expect("install must call shared lifecycle cleanup");

    // Every guard acquisition must be a closure passed to the bounded wait —
    // no bare one-shot try_acquire() that fails while a scheduler tick holds
    // the slot. The helper itself owns the short sleep loop and 2 s deadline.
    let mut acquisitions = 0;
    for (pos, _) in source.match_indices(".try_acquire()") {
        let prefix = &source[pos.saturating_sub(96)..pos];
        assert!(
            prefix.contains("wait_for_permit(deadline, || "),
            "one-shot try_acquire() must not block Install while a scheduler holds a guard"
        );
        acquisitions += 1;
    }
    assert_eq!(acquisitions, 3, "all three domain guards must use bounded wait");
    assert!(
        source.contains("Duration::from_secs(2)"),
        "bounded wait must have a two-second deadline"
    );
    assert!(
        source.contains("tokio::time::sleep"),
        "bounded wait must sleep briefly between retries"
    );

    for guard in ["connection_guard", "probe_guard", "mutation_guard"] {
        let acquired = try_acquire_position(&source, guard);
        assert!(
            acquired < cleanup,
            "{guard} bounded acquisition must finish before teardown starts"
        );
    }
}

#[test]
fn install_source_captures_profile_before_cleanup_and_reconnects_after_abort() {
    let source = update_source();
    let capture = source
        .find("connection.profile_id")
        .expect("install must capture the live profile id before teardown");
    let cleanup = source
        .find("cleanup(")
        .expect("install must call shared lifecycle cleanup");
    let abort = source
        .find("abort_shutdown()")
        .expect("failed install must release the shutdown gate");
    let reconnect = source
        .find("connect_impl(&state, Some(id))")
        .expect("failed install must restore the captured VPN connection");
    let error = source
        .find("Err(CommandError::Update)")
        .expect("failed install must map to the update error");
    assert!(
        capture < cleanup,
        "profile id must be captured while the connection is still live"
    );
    assert!(
        abort < reconnect && reconnect < error,
        "reconnect must be attempted after releasing the gate, before returning the error"
    );
}
