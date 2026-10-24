//! Compositor-loss policy: terminate cleanly; a fresh shell instance reconnects.
//! Only the private compositor Child created by nested::driver is stopped here.
use super::*;
use serde_json::{json, Value};
fn request(runtime: &Path, function: &str) -> Result<Value, Box<dyn Error>> {
    use std::io::{Read, Write};
    let mut stream = std::os::unix::net::UnixStream::connect(runtime.join("functional.sock"))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(&serde_json::to_vec(&hyprv_core::ipc::Request {
        target: "e2e".into(),
        function: function.into(),
        args: vec![],
    })?)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    let response: hyprv_core::ipc::Response = serde_json::from_slice(&bytes)?;
    if !response.ok {
        return Err(response.result.into());
    }
    Ok(serde_json::from_str(&response.result)?)
}
fn ready(runtime: &Path, child: &mut Child) -> Result<Value, Box<dyn Error>> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(format!("probe exited early: {status}").into());
        }
        if let Ok(state) = request(runtime, "snapshot") {
            if state["outputs"].as_array().is_some_and(|v| !v.is_empty())
                && state["surfaces"].as_array().is_some_and(|v| v.len() >= 2)
            {
                return Ok(state);
            }
        }
        if Instant::now() > until {
            return Err("probe readiness timed out".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn wait(child: &mut Child) -> Result<std::process::ExitStatus, Box<dyn Error>> {
    let until = Instant::now() + Duration::from_secs(6);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() > until {
            let _ = child.kill();
            let _ = child.wait();
            return Err("shell failed to exit after connection loss/quit".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
pub(super) fn run(
    compositor: &mut Compositor,
    compositor_command: &mut Command,
    shell_command: &mut Command,
    runtime: &Path,
    socket: &Path,
    signature: &std::ffi::OsStr,
    directory: &Path,
) -> Vec<Value> {
    let mut rows = vec![];
    let result = (|| -> Result<(), Box<dyn Error>> {
        let _ = fs::remove_file(runtime.join("functional.sock"));
        shell_command
            .env("HYPRV_FUNCTIONAL_PROBE", "1")
            .stdout(fs::File::create(directory.join("disconnect-shell.log"))?)
            .stderr(fs::File::create(directory.join("disconnect-shell.err"))?);
        let mut shell = Compositor(shell_command.spawn()?);
        let before = ready(runtime, &mut shell.0)?;
        compositor.0.kill()?;
        compositor.0.wait()?;
        let status = wait(&mut shell.0)?;
        let stderr = fs::read_to_string(directory.join("disconnect-shell.err"))?;
        let clean = status.code() == Some(1)
            && !stderr.contains("panicked")
            && !runtime.join("hyprv/shell.sock").exists();
        rows.push(json!({"id":"compositor.disconnect","source":"hyprv-ui Application connection-loss policy","status":if clean{"PASS"}else{"FAIL"},"detail":{"exit_code":status.code(),"no_panic":!stderr.contains("panicked"),"ipc_removed":!runtime.join("hyprv/shell.sock").exists(),"outputs_before":before["outputs"],"policy":"Exit with Wayland diagnostic; no in-process reconnect loop"}}));
        let _ = fs::remove_file(socket);
        let _ = fs::remove_file(PathBuf::from(format!("{}.lock", socket.display())));
        let _ = fs::remove_file(runtime.join("functional.sock"));
        let _ = fs::remove_dir_all(runtime.join("hypr").join(signature));
        compositor.0 = compositor_command.spawn()?;
        let until = Instant::now() + Duration::from_secs(15);
        let new_signature = loop {
            if let Some(status) = compositor.0.try_wait()? {
                return Err(format!("restarted compositor exited: {status}").into());
            }
            if socket.exists() {
                if let Some(entry) = fs::read_dir(runtime.join("hypr"))
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .find(|e| e.path().join(".socket.sock").exists())
                {
                    break entry.file_name();
                }
            }
            if Instant::now() > until {
                return Err("restarted compositor socket timed out".into());
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        shell_command
            .env("HYPRLAND_INSTANCE_SIGNATURE", &new_signature)
            .stdout(fs::File::create(directory.join("reconnect-shell.log"))?)
            .stderr(fs::File::create(directory.join("reconnect-shell.err"))?);
        let mut fresh = Compositor(shell_command.spawn()?);
        let state = ready(runtime, &mut fresh.0)?;
        let _ = request(runtime, "quit");
        let status = wait(&mut fresh.0)?;
        let pass = status.success() && state["actions"] == json!([]);
        rows.push(json!({"id":"compositor.restart-fresh-reconnect","source":"hyprv-ui Application lifecycle","status":if pass{"PASS"}else{"FAIL"},"detail":{"exit_code":status.code(),"output_count":state["outputs"].as_array().map(Vec::len),"surface_count":state["surfaces"].as_array().map(Vec::len),"actions":state["actions"],"policy":"Supervisor-style fresh instance connects to restarted compositor; no stale native objects"}}));
        Ok(())
    })();
    if let Err(e) = result {
        rows.push(json!({"id":"compositor.restart-fresh-reconnect","source":"private compositor lifecycle","status":"FAIL","error":e.to_string()}));
    }
    rows
}
