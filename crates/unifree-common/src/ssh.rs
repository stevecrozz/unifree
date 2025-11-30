use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info, warn};
use anyhow::Result;
use std::os::unix::fs::PermissionsExt;

/// Generate a random 32-character hex auth key
pub fn generate_auth_key() -> String {
    use rand::Rng;
    
    let mut rng = rand::rng();
    let mut key = [0u8; 16];
    rng.fill(&mut key);
    
    hex::encode(key)
}

/// Execute a command via SSH using the system binary
/// Returns (exit_code, stdout)
pub async fn execute_ssh_command(
    ip: std::net::IpAddr,
    port: u16,
    user: &str,
    pass: &str,
    command: &str,
) -> Result<(Option<i32>, String)> {
    debug!("Connecting to {}:{} as {} via system ssh", ip, port, user);

    // Setup temporary files for SSH_ASKPASS
    let uuid = uuid::Uuid::new_v4().to_string();
    let temp_dir = std::env::temp_dir();
    
    let pass_path = temp_dir.join(format!("unifree_pass_{}", uuid));
    let script_path = temp_dir.join(format!("unifree_askpass_{}.sh", uuid));

    // Write password to file
    tokio::fs::write(&pass_path, pass).await?;

    // Write wrapper script
    let script_content = format!("#!/bin/sh\ncat \"{}\"\n", pass_path.display());
    tokio::fs::write(&script_path, &script_content).await?;

    // Make script executable
    let mut perms = std::fs::metadata(&script_path)?.permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(&script_path, perms)?;

    info!("Executing SSH command: {}", command);

    let child = Command::new("ssh")
        .env("SSH_ASKPASS", &script_path)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env("DISPLAY", ":0") // Required to trigger askpass in some versions
        .arg("-o").arg("StrictHostKeyChecking=no")
        .arg("-o").arg("UserKnownHostsFile=/dev/null")
        .arg("-o").arg("LogLevel=ERROR")
        .arg("-p").arg(port.to_string())
        .arg("-l").arg(user)
        .arg(ip.to_string())
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let result = match child {
        Ok(child) => {
            let output = child.wait_with_output().await?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            
            if !stderr.is_empty() {
                debug!("SSH stderr: {}", stderr);
            }
            
            let code = output.status.code();
            Ok((code, stdout))
        }
        Err(e) => Err(anyhow::anyhow!("Failed to spawn ssh: {}", e)),
    };

    // Cleanup temp files
    let _ = tokio::fs::remove_file(pass_path).await;
    let _ = tokio::fs::remove_file(script_path).await;

    result
}
