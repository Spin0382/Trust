//! The Windows-only half of self-update: the detached helper that replaces the running binary
//! after its parent exits.
//!
//! `update::install` cannot stop and restart this process, and Windows will not let a running
//! image replace its own file. It therefore stages the verified binary beside the target and
//! starts that staged copy with `__apply-update <parent-pid> <staged> <target> <original args>`.
//! The helper is the new release, so it can rename its own image into place once the old target
//! has been moved aside; Windows permits renaming a running executable, only deleting it is
//! denied. This file drives that hidden subcommand directly, with a real short-lived parent and a
//! real copy of the binary as the staged helper, so it proves the ordering (wait, then swap), the
//! replacement, the cleanup, and that the relaunched process receives the arguments and working
//! directory the helper was given.
//!
//! The unit tests for argument construction and for the retry/restore decision live in
//! `update.rs` itself, because they compile and run on every platform; this file is empty
//! elsewhere.
#![cfg(windows)]

mod support;

use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const HELPER_TIMEOUT: Duration = Duration::from_secs(60);

/// A parent that lives for a couple of seconds, so the helper has something real to wait for.
/// `ping` is used instead of a shell `sleep` because Windows' own `timeout` refuses redirected
/// stdin and exits immediately.
fn spawn_short_lived_parent(working_dir: &Path) -> Child {
    Command::new("cmd")
        .args(["/C", "ping", "-n", "3", "127.0.0.1"])
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn a short-lived dummy parent")
}

fn wait_for_helper(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + HELPER_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().expect("poll the update helper") {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the update helper did not finish within {HELPER_TIMEOUT:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Wait for the relaunched process to write the expected worlds listing. The helper exits as soon
/// as it has spawned that process, so the file can still be empty at this point.
fn wait_for_output(path: &Path, errors: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(text) = std::fs::read_to_string(path)
            && text.contains("UpdateApplyFixture")
        {
            return text;
        }
        if Instant::now() >= deadline {
            let stderr = std::fs::read_to_string(errors).unwrap_or_default();
            panic!(
                "the relaunched process never wrote {}; its stderr was: {stderr}",
                path.display()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_detached_helper_waits_for_its_parent_replaces_the_target_and_relaunches_with_context() {
    let dir = support::scratch_dir("update-apply");
    std::fs::create_dir_all(dir.join("worlds")).expect("create the scratch directory");

    let target = dir.join("terrustia.exe");
    std::fs::write(&target, b"old binary contents").expect("write the old target");

    // The real binary, as both the helper and the replacement. This is exactly what `install`
    // stages: the next release, which contains this subcommand.
    let staged = dir.join("staged.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_terrustia"), &staged).expect("stage the helper binary");
    let staged_bytes = std::fs::read(&staged).expect("read the staged binary");

    // `--worlds` lists this directory relative to the working directory, so finding the fixture
    // is what proves the relaunched process kept the helper's cwd. A directory with a `.wld`
    // extension is listed and then fails to load, which is enough without feeding arbitrary bytes
    // to the world parser.
    std::fs::create_dir_all(dir.join("worlds").join("UpdateApplyFixture.wld"))
        .expect("write a worlds fixture");

    let output_path = dir.join("relaunched-output.txt");
    let error_path = dir.join("relaunched-error.txt");
    let output = std::fs::File::create(&output_path).expect("create the output file");
    let errors = std::fs::File::create(&error_path).expect("create the error file");

    let mut parent = spawn_short_lived_parent(&dir);
    let parent_pid = parent.id();

    let mut helper = Command::new(&staged)
        .arg(terrustia::update::APPLY_UPDATE_SUBCOMMAND)
        .arg(parent_pid.to_string())
        .arg(&staged)
        .arg(&target)
        // The relaunched binary parses these exactly as the parent would have. `--worlds` returns
        // before the config file is read; the other two are here to prove the whole argument list
        // survives, not just its first word.
        .args(["--worlds", "--config", "custom.toml", "--panel"])
        .current_dir(&dir)
        // If argument forwarding ever regressed to an empty command line, the server path would
        // fail here instead of starting a world generation the test would have to chase.
        .env("TERRUSTIA_LISTEN", "not-an-address")
        .env("TERRUSTIA_UPDATE_CHECK_ENABLED", "false")
        .env("TERRUSTIA_UPNP_ENABLED", "false")
        .env_remove("TERRUSTIA_LOG")
        .stdin(Stdio::null())
        .stdout(Stdio::from(output))
        .stderr(Stdio::from(errors))
        .spawn()
        .expect("spawn the update helper");

    // While the dummy parent is still alive, the helper must not have swapped the target: it is
    // supposed to wait for the process that owns the old file to finish first. If the parent
    // already exited (a very fast machine), skip the assertion rather than making it flaky.
    std::thread::sleep(Duration::from_millis(300));
    if parent.try_wait().expect("poll the dummy parent").is_none() {
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"old binary contents",
            "the helper replaced the target before its parent exited"
        );
    }

    let status = wait_for_helper(&mut helper);
    if !status.success() {
        let log = std::fs::read_to_string(dir.join("terrustia.exe.update.log")).unwrap_or_default();
        let stderr = std::fs::read_to_string(&error_path).unwrap_or_default();
        panic!("the helper exited with {status}; log: {log}; stderr: {stderr}");
    }
    let _ = parent.wait();

    assert_eq!(
        std::fs::read(&target).unwrap(),
        staged_bytes,
        "the target must be the verified staged binary after the helper finishes"
    );
    assert!(
        !staged.exists(),
        "the staged file is the helper's own image; renaming it into place must consume it"
    );

    let leftovers: Vec<String> = std::fs::read_dir(&dir)
        .expect("list the scratch directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".old-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "backup files left behind: {leftovers:?}"
    );

    let output = wait_for_output(&output_path, &error_path);
    assert!(
        output.contains("UpdateApplyFixture"),
        "the relaunched process did not run from the helper's working directory: {output}"
    );
    assert!(
        !output.contains("cannot parse"),
        "the relaunched process did not receive the parent's arguments: {output}"
    );

    let log = std::fs::read_to_string(dir.join("terrustia.exe.update.log"))
        .expect("the helper appends to an update log");
    assert!(log.contains("replaced the target"), "log: {log}");
    assert!(log.contains("relaunched the replacement"), "log: {log}");

    let _ = std::fs::remove_dir_all(&dir);
}
