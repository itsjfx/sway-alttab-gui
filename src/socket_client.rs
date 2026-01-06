use crate::ipc::{get_socket_path, IpcCommand, IpcResponse};
use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

/// Send a command to the daemon and get the response
pub fn send_command(command: IpcCommand) -> Result<IpcResponse> {
    let socket_path = get_socket_path()?;

    let mut stream = UnixStream::connect(&socket_path).with_context(|| {
        format!(
            "Failed to connect to daemon at {}. Is the daemon running?",
            socket_path.display()
        )
    })?;

    // Set timeouts
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    // Send command as simple string
    writeln!(stream, "{}", command)?;
    stream.flush()?;

    // Read response
    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    let response: IpcResponse =
        serde_json::from_str(&response_line).context("Failed to parse daemon response")?;

    Ok(response)
}

/// Send command and print result, exit with appropriate code
pub fn send_command_and_exit(command: IpcCommand) -> ! {
    match send_command(command) {
        Ok(IpcResponse::Ok) => {
            std::process::exit(0);
        }
        Ok(IpcResponse::Status {
            switching,
            window_count,
            current_index,
        }) => {
            println!("Daemon Status:");
            println!("  Switching: {}", switching);
            println!("  Window count: {}", window_count);
            if let Some(idx) = current_index {
                println!("  Current index: {}", idx);
            }
            std::process::exit(0);
        }
        Ok(IpcResponse::Error(e)) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
