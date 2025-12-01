use anyhow::{Context, Result};
use ssh2::Session;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use tokio::task;
use tracing::{debug, info};

/// Generate a random 32-character hex auth key
pub fn generate_auth_key() -> String {
    use rand::Rng;

    let mut rng = rand::rng();
    let mut key = [0u8; 16];
    rng.fill(&mut key);

    hex::encode(key)
}

/// Execute a command via SSH using libssh2
/// Returns (exit_code, stdout)
pub async fn execute_ssh_command(
    ip: std::net::IpAddr,
    port: u16,
    user: &str,
    pass: &str,
    command: &str,
) -> Result<(Option<i32>, String)> {
    debug!("Connecting to {}:{} as {} via libssh2", ip, port, user);
    let addr = SocketAddr::new(ip, port);
    let user = user.to_string();
    let pass = pass.to_string();
    let command = command.to_string();

    task::spawn_blocking(move || -> Result<(Option<i32>, String)> {
        let tcp = TcpStream::connect(addr).with_context(|| format!("connect to {}", addr))?;
        tcp.set_read_timeout(Some(std::time::Duration::from_secs(30)))
            .ok();
        tcp.set_write_timeout(Some(std::time::Duration::from_secs(30)))
            .ok();

        let mut session = Session::new().context("create ssh session")?;
        session.set_tcp_stream(tcp);
        session.handshake().context("ssh handshake")?;
        session
            .userauth_password(&user, &pass)
            .context("password authentication")?;

        if !session.authenticated() {
            anyhow::bail!("SSH authentication failed for {}", user);
        }

        info!("Executing SSH command: {}", command);
        let mut channel = session.channel_session().context("open channel")?;
        channel.exec(&command).context("exec command")?;

        let mut stdout = String::new();
        channel.read_to_string(&mut stdout).context("read stdout")?;

        let mut stderr = String::new();
        channel.stderr().read_to_string(&mut stderr).ok();
        if !stderr.trim().is_empty() {
            debug!("SSH stderr: {}", stderr.trim());
        }

        channel.wait_close().ok();
        let exit_status = channel.exit_status().ok();

        Ok((exit_status, stdout))
    })
    .await
    .context("ssh task join")?
}
